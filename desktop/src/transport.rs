//! Direct TCP exchange and local cache; this is not a BitTorrent implementation.
use crate::discovery::{lan_unix_secs, local_lan_ip};
use serde::{Deserialize, Serialize};
use snartnet_core::{SignedMessage, SignedPost, SignedProfile};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_PACKET_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmProfileBlob {
    pub profile: SignedProfile,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwarmPostsBlob {
    pub posts: Vec<SignedPost>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwarmInboxBlob {
    pub messages: Vec<SignedMessage>,
    pub updated_at: u64,
}

pub trait NetworkTransport {
    fn load_profile(&self, fingerprint: &str) -> Option<SwarmProfileBlob>;
    fn save_profile(&self, fingerprint: &str, blob: &SwarmProfileBlob) -> Result<(), String>;

    fn load_posts(&self, fingerprint: &str) -> Option<SwarmPostsBlob>;
    fn save_posts(&self, fingerprint: &str, blob: &SwarmPostsBlob) -> Result<(), String>;

    fn load_inbox(&self, recipient_fingerprint: &str) -> Option<SwarmInboxBlob>;
    fn save_inbox(&self, recipient_fingerprint: &str, blob: &SwarmInboxBlob) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TransportRequest {
    GetProfile {
        fingerprint: String,
    },
    PutProfile {
        fingerprint: String,
        blob: Box<SwarmProfileBlob>,
    },
    GetPosts {
        fingerprint: String,
    },
    PutPosts {
        fingerprint: String,
        blob: SwarmPostsBlob,
    },
    GetInbox {
        recipient_fingerprint: String,
    },
    PutInbox {
        recipient_fingerprint: String,
        blob: SwarmInboxBlob,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TransportResponse {
    Ok,
    Profile { blob: Option<Box<SwarmProfileBlob>> },
    Posts { blob: Option<SwarmPostsBlob> },
    Inbox { blob: Option<SwarmInboxBlob> },
    Err { message: String },
}

#[derive(Clone)]
pub struct TcpSwarmTransport {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    swarm_dir: PathBuf,
    bind_addr: SocketAddr,
    base_peers: Vec<SocketAddr>,
    peers: Mutex<Vec<SocketAddr>>,
    /// Serializes read/merge/replace inbox updates from the listener and sync worker.
    writes: Mutex<()>,
}

impl TcpSwarmTransport {
    #[cfg(test)]
    pub(crate) fn for_test(root: &Path) -> Self {
        std::fs::create_dir_all(root).unwrap();
        Self {
            inner: Arc::new(Inner {
                swarm_dir: root.into(),
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                base_peers: Vec::new(),
                peers: Mutex::new(Vec::new()),
                writes: Mutex::new(()),
            }),
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let bind_addr = std::env::var("SNARTNET_BIND")
            .ok()
            .and_then(|s| s.parse::<SocketAddr>().ok())
            .unwrap_or_else(|| "0.0.0.0:47470".parse().expect("valid default bind addr"));

        let peers = std::env::var("SNARTNET_PEERS")
            .ok()
            .map(|v| {
                v.split(',')
                    .filter_map(|p| p.trim().parse::<SocketAddr>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let swarm_dir = swarm_root_dir()?;

        Ok(Self {
            inner: Arc::new(Inner {
                swarm_dir,
                bind_addr,
                base_peers: peers,
                peers: Mutex::new(Vec::new()),
                writes: Mutex::new(()),
            }),
        })
    }

    pub fn set_peers(&self, peers: Vec<SocketAddr>) {
        let mut guard = self.inner.peers.lock().unwrap();
        *guard = peers;
    }

    pub fn peer_snapshot(&self) -> Vec<SocketAddr> {
        let mut peers = self.inner.base_peers.clone();
        for peer in self.inner.peers.lock().unwrap().iter().copied() {
            if !peers.contains(&peer) {
                peers.push(peer);
            }
        }
        peers
    }

    /// Bind synchronously so startup can report a port conflict instead of pretending to listen.
    pub fn start_server(&self) -> Result<SocketAddr, String> {
        let listener = TcpListener::bind(self.inner.bind_addr)
            .map_err(|e| format!("Cannot listen on {}: {e}", self.inner.bind_addr))?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let this = self.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut buf = Vec::new();
                let response = match (&mut stream)
                    .take(MAX_PACKET_BYTES + 1)
                    .read_to_end(&mut buf)
                {
                    Ok(_) if buf.len() as u64 <= MAX_PACKET_BYTES => {
                        match serde_json::from_slice(&buf) {
                            Ok(request) => this.handle_request(request),
                            Err(e) => TransportResponse::Err {
                                message: e.to_string(),
                            },
                        }
                    }
                    _ => TransportResponse::Err {
                        message: "Request too large or timed out".into(),
                    },
                };
                if let Ok(payload) = serde_json::to_vec(&response) {
                    let _ = stream.write_all(&payload);
                }
                let _ = stream.shutdown(Shutdown::Both);
            }
        });
        Ok(address)
    }

    pub fn advertised_addr(&self) -> Option<String> {
        let bind = self.inner.bind_addr;
        let ip = if bind.ip().is_unspecified() {
            local_lan_ip()?
        } else {
            bind.ip()
        };
        Some(SocketAddr::new(ip, bind.port()).to_string())
    }

    pub fn swarm_dir(&self) -> &Path {
        &self.inner.swarm_dir
    }

    /// Publish a durable local snapshot. Acknowledgement means a peer stored it, not that it was read.
    pub fn relay_profile(&self, profile: &SignedProfile) {
        self.fanout_put(&TransportRequest::PutProfile {
            fingerprint: profile.profile.fingerprint.clone(),
            blob: Box::new(SwarmProfileBlob {
                profile: profile.clone(),
                updated_at: lan_unix_secs(),
            }),
        });
    }

    pub fn relay_posts(&self, fingerprint: &str, posts: Vec<SignedPost>) {
        self.fanout_put(&TransportRequest::PutPosts {
            fingerprint: fingerprint.into(),
            blob: SwarmPostsBlob {
                posts,
                updated_at: lan_unix_secs(),
            },
        });
    }

    pub fn relay_message(&self, message: &SignedMessage) -> bool {
        self.fanout_put(&TransportRequest::PutInbox {
            recipient_fingerprint: message.message.recipient_fingerprint.clone(),
            blob: SwarmInboxBlob {
                messages: vec![message.clone()],
                updated_at: lan_unix_secs(),
            },
        }) > 0
    }

    fn handle_request(&self, req: TransportRequest) -> TransportResponse {
        match req {
            TransportRequest::GetProfile { fingerprint } => TransportResponse::Profile {
                blob: self.load_profile_local(&fingerprint).map(Box::new),
            },
            TransportRequest::PutProfile { fingerprint, blob } => {
                match self.save_profile_local(&fingerprint, &blob) {
                    Ok(_) => TransportResponse::Ok,
                    Err(e) => TransportResponse::Err { message: e },
                }
            }
            TransportRequest::GetPosts { fingerprint } => TransportResponse::Posts {
                blob: self.load_posts_local(&fingerprint),
            },
            TransportRequest::PutPosts { fingerprint, blob } => {
                match self.save_posts_local(&fingerprint, &blob) {
                    Ok(_) => TransportResponse::Ok,
                    Err(e) => TransportResponse::Err { message: e },
                }
            }
            TransportRequest::GetInbox {
                recipient_fingerprint,
            } => TransportResponse::Inbox {
                blob: self.load_inbox_local(&recipient_fingerprint),
            },
            TransportRequest::PutInbox {
                recipient_fingerprint,
                mut blob,
            } => {
                dedupe_inbox(&mut blob);
                match self.save_inbox_local(&recipient_fingerprint, &blob) {
                    Ok(_) => TransportResponse::Ok,
                    Err(e) => TransportResponse::Err { message: e },
                }
            }
        }
    }

    fn request_peer(&self, peer: SocketAddr, req: &TransportRequest) -> Option<TransportResponse> {
        let mut stream = TcpStream::connect_timeout(&peer, Duration::from_millis(700)).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(900)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(900)));

        let payload = serde_json::to_vec(req).ok()?;
        stream.write_all(&payload).ok()?;
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Write);

        let mut out = Vec::new();
        stream
            .take(MAX_PACKET_BYTES + 1)
            .read_to_end(&mut out)
            .ok()?;
        if out.len() as u64 > MAX_PACKET_BYTES {
            return None;
        }
        serde_json::from_slice::<TransportResponse>(&out).ok()
    }

    fn fanout_put(&self, req: &TransportRequest) -> usize {
        self.peer_snapshot()
            .into_iter()
            .filter(|peer| matches!(self.request_peer(*peer, req), Some(TransportResponse::Ok)))
            .count()
    }

    fn profile_path(&self, fingerprint: &str) -> PathBuf {
        self.inner
            .swarm_dir
            .join(format!("profile_{}.json", sanitize_component(fingerprint)))
    }

    fn posts_path(&self, fingerprint: &str) -> PathBuf {
        self.inner
            .swarm_dir
            .join(format!("posts_{}.json", sanitize_component(fingerprint)))
    }

    fn inbox_path(&self, recipient_fingerprint: &str) -> PathBuf {
        self.inner.swarm_dir.join(format!(
            "inbox_{}.json",
            sanitize_component(recipient_fingerprint)
        ))
    }

    fn load_profile_local(&self, fingerprint: &str) -> Option<SwarmProfileBlob> {
        load_json_file(&self.profile_path(fingerprint))
            .ok()
            .flatten()
    }

    fn save_profile_local(&self, fingerprint: &str, blob: &SwarmProfileBlob) -> Result<(), String> {
        if blob.profile.profile.fingerprint != fingerprint
            || !blob.profile.verify().unwrap_or(false)
        {
            return Err("Profile identity or signature is invalid".into());
        }
        let _guard = self.inner.writes.lock().unwrap();
        if let Some(existing) = self.load_profile_local(fingerprint) {
            if existing.profile.profile.updated_at > blob.profile.profile.updated_at {
                return Ok(());
            }
        }
        save_json_file(&self.profile_path(fingerprint), blob)
    }

    fn load_posts_local(&self, fingerprint: &str) -> Option<SwarmPostsBlob> {
        load_json_file(&self.posts_path(fingerprint)).ok().flatten()
    }

    fn save_posts_local(&self, fingerprint: &str, blob: &SwarmPostsBlob) -> Result<(), String> {
        let profile = self
            .load_profile_local(fingerprint)
            .ok_or("Author profile is missing")?;
        if blob.posts.iter().any(|post| {
            post.post.author_fingerprint != fingerprint
                || !post
                    .verify(&profile.profile.profile.public_key)
                    .unwrap_or(false)
        }) {
            return Err("Post signature is invalid".into());
        }
        let _guard = self.inner.writes.lock().unwrap();
        let mut merged = self.load_posts_local(fingerprint).unwrap_or_default();
        merged.posts.extend(blob.posts.clone());
        let mut seen = std::collections::HashSet::new();
        merged
            .posts
            .retain(|post| seen.insert(post.post.id.clone()));
        merged
            .posts
            .sort_by_key(|post| std::cmp::Reverse(post.post.created_at));
        merged.updated_at = merged.updated_at.max(blob.updated_at);
        save_json_file(&self.posts_path(fingerprint), &merged)
    }

    fn load_inbox_local(&self, recipient_fingerprint: &str) -> Option<SwarmInboxBlob> {
        load_json_file(&self.inbox_path(recipient_fingerprint))
            .ok()
            .flatten()
    }

    fn save_inbox_local(
        &self,
        recipient_fingerprint: &str,
        blob: &SwarmInboxBlob,
    ) -> Result<(), String> {
        for message in &blob.messages {
            let sender = self
                .load_profile_local(&message.message.sender_fingerprint)
                .ok_or("Sender profile is missing")?;
            if message.message.recipient_fingerprint != recipient_fingerprint
                || !message
                    .verify(&sender.profile.profile.public_key)
                    .unwrap_or(false)
            {
                return Err("Message signature or recipient is invalid".into());
            }
        }
        let _guard = self.inner.writes.lock().unwrap();
        let mut merged = self
            .load_inbox_local(recipient_fingerprint)
            .unwrap_or_default();
        merged.messages.extend(blob.messages.clone());
        merged.updated_at = merged.updated_at.max(blob.updated_at);
        dedupe_inbox(&mut merged);
        save_json_file(&self.inbox_path(recipient_fingerprint), &merged)
    }
}

impl NetworkTransport for TcpSwarmTransport {
    fn load_profile(&self, fingerprint: &str) -> Option<SwarmProfileBlob> {
        // Always refresh from peers: returning the first cache hit prevented profile updates forever.
        for peer in self.peer_snapshot() {
            if let Some(TransportResponse::Profile { blob: Some(blob) }) = self.request_peer(
                peer,
                &TransportRequest::GetProfile {
                    fingerprint: fingerprint.into(),
                },
            ) {
                let _ = self.save_profile_local(fingerprint, &blob);
            }
        }
        self.load_profile_local(fingerprint)
    }

    fn save_profile(&self, fingerprint: &str, blob: &SwarmProfileBlob) -> Result<(), String> {
        self.save_profile_local(fingerprint, blob)
    }

    fn load_posts(&self, fingerprint: &str) -> Option<SwarmPostsBlob> {
        for peer in self.peer_snapshot() {
            if let Some(TransportResponse::Posts { blob: Some(blob) }) = self.request_peer(
                peer,
                &TransportRequest::GetPosts {
                    fingerprint: fingerprint.into(),
                },
            ) {
                let _ = self.save_posts_local(fingerprint, &blob);
            }
        }
        self.load_posts_local(fingerprint)
    }

    fn save_posts(&self, fingerprint: &str, blob: &SwarmPostsBlob) -> Result<(), String> {
        self.save_posts_local(fingerprint, blob)
    }

    fn load_inbox(&self, recipient_fingerprint: &str) -> Option<SwarmInboxBlob> {
        for peer in self.peer_snapshot() {
            if let Some(TransportResponse::Inbox { blob: Some(blob) }) = self.request_peer(
                peer,
                &TransportRequest::GetInbox {
                    recipient_fingerprint: recipient_fingerprint.into(),
                },
            ) {
                // Validate separately: a bad envelope must not discard other valid messages.
                for message in blob.messages {
                    let single = SwarmInboxBlob {
                        messages: vec![message],
                        updated_at: blob.updated_at,
                    };
                    let _ = self.save_inbox_local(recipient_fingerprint, &single);
                }
            }
        }
        self.load_inbox_local(recipient_fingerprint)
    }

    fn save_inbox(&self, recipient_fingerprint: &str, blob: &SwarmInboxBlob) -> Result<(), String> {
        self.save_inbox_local(recipient_fingerprint, blob)
    }
}

fn swarm_root_dir() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("SNARTNET_HOME") {
        let root = PathBuf::from(root).join("swarm");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        return Ok(root);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory".to_string())?;
    let root = PathBuf::from(home).join(".snartnet").join("swarm");
    std::fs::create_dir_all(&root).map_err(|e| format!("failed to create swarm dir: {e}"))?;
    Ok(root)
}

pub(crate) fn sanitize_component(s: &str) -> String {
    const UNSAFE: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '%'];
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if UNSAFE.contains(&ch) || ch.is_control() {
            for byte in ch.to_string().as_bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn load_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let value = serde_json::from_str::<T>(&text).map_err(|e| format!("json parse failed: {e}"))?;
    Ok(Some(value))
}

fn save_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text =
        serde_json::to_string_pretty(value).map_err(|e| format!("json write failed: {e}"))?;
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing parent directory")?)
            .map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn dedupe_inbox(inbox: &mut SwarmInboxBlob) {
    let mut seen = std::collections::HashSet::new();
    // Include the signature in the transport dedupe key. An invalid envelope with a guessed
    // message ID must not suppress the authentic envelope before the recipient verifies it.
    inbox
        .messages
        .retain(|m| seen.insert((m.message.id.clone(), m.signature.clone())));
    inbox.messages.sort_by_key(|m| m.message.created_at);
}
