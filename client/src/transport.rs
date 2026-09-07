//! Direct TCP fallback plus BitTorrent/DHT publication and profile retrieval.
use crate::{
    dht::DhtNode,
    discovery::{lan_unix_secs, local_lan_ip},
    model::Contact,
    torrent::TorrentNode,
};
use serde::{Deserialize, Serialize};
use snartnet_core::{KeyPair, Message, MessageType, SignedMessage, SignedPost, SignedProfile};
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
    fn load_profile_for_contact(&self, contact: &Contact) -> Option<SwarmProfileBlob> {
        self.load_profile(&contact.fingerprint)
    }
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

struct Inner {
    swarm_dir: PathBuf,
    bind_addr: SocketAddr,
    base_peers: Vec<SocketAddr>,
    peers: Mutex<Vec<SocketAddr>>,
    /// Serializes read/merge/replace inbox updates from the listener and sync worker.
    writes: Mutex<()>,
    listening_addr: Mutex<Option<SocketAddr>>,
    torrent: Option<Arc<TorrentNode>>,
    dht: Mutex<Option<Arc<DhtNode>>>,
    identity: Mutex<Option<KeyPair>>,
}

impl TcpSwarmTransport {
    pub fn for_test(root: &Path) -> Self {
        std::fs::create_dir_all(root).unwrap();
        Self {
            inner: Arc::new(Inner {
                swarm_dir: root.into(),
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                base_peers: Vec::new(),
                peers: Mutex::new(Vec::new()),
                writes: Mutex::new(()),
                listening_addr: Mutex::new(None),
                torrent: None,
                dht: Mutex::new(None),
                identity: Mutex::new(None),
            }),
        }
    }

    pub fn new(root: &Path, bind_addr: SocketAddr) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Arc::new(Inner {
                swarm_dir: root.into(),
                bind_addr,
                base_peers: Vec::new(),
                peers: Mutex::new(Vec::new()),
                writes: Mutex::new(()),
                listening_addr: Mutex::new(None),
                torrent: TorrentNode::open(
                    root.join("torrent"),
                    bind_addr.port().saturating_add(1),
                )
                .ok()
                .map(Arc::new),
                dht: Mutex::new(None),
                identity: Mutex::new(None),
            }),
        })
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
                swarm_dir: swarm_dir.clone(),
                bind_addr,
                base_peers: peers,
                peers: Mutex::new(Vec::new()),
                writes: Mutex::new(()),
                listening_addr: Mutex::new(None),
                torrent: TorrentNode::open(
                    swarm_dir.join("torrent"),
                    bind_addr.port().saturating_add(1),
                )
                .ok()
                .map(Arc::new),
                dht: Mutex::new(None),
                identity: Mutex::new(None),
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

    pub fn distributed_status(&self) -> (Option<crate::dht::DhtStatus>, Option<crate::torrent::TorrentStatus>) {
        let dht = self.inner.dht.lock().ok().and_then(|node| node.as_ref().map(|node| node.status()));
        let torrent = self.inner.torrent.as_ref().map(|node| node.status());
        (dht, torrent)
    }

    /// Attach the local identity so the transport can publish signed BEP-44
    /// mailbox/feed descriptors. The secret key never leaves the DHT node.
    pub fn set_identity(&self, keypair: &KeyPair) {
        *self.inner.identity.lock().unwrap() = Some(keypair.clone());
        if self.inner.dht.lock().unwrap().is_none() {
            let port = self.inner.bind_addr.port().saturating_add(2);
            *self.inner.dht.lock().unwrap() = DhtNode::open(keypair, port).ok().map(Arc::new);
        }
    }

    /// Bind synchronously so startup can report a port conflict instead of pretending to listen.
    pub fn start_server(&self) -> Result<SocketAddr, String> {
        let listener = TcpListener::bind(self.inner.bind_addr)
            .map_err(|e| format!("Cannot listen on {}: {e}", self.inner.bind_addr))?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        *self.inner.listening_addr.lock().unwrap() = Some(address);
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
        let bind = self
            .inner
            .listening_addr
            .lock()
            .unwrap()
            .unwrap_or(self.inner.bind_addr);
        if bind.port() == 0 {
            return None;
        }
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
        if let Some(torrent) = &self.inner.torrent {
            let object_id = format!("profile-{}", profile.profile.fingerprint);
            if let Ok(bytes) = serde_json::to_vec(profile) {
                if let Ok(magnet) = torrent.publish(&object_id, &bytes) {
                    if let Some(dht) = self.inner.dht.lock().unwrap().as_ref() {
                        if let Ok(value) = serde_json::to_vec(
                            &serde_json::json!({"magnet": magnet, "object_id": object_id}),
                        ) {
                            let _ = dht.publish(
                                "snartnet/profile",
                                &[&profile.profile.fingerprint],
                                &value,
                            );
                        }
                    }
                }
            }
        }
        self.fanout_put(&TransportRequest::PutProfile {
            fingerprint: profile.profile.fingerprint.clone(),
            blob: Box::new(SwarmProfileBlob {
                profile: profile.clone(),
                updated_at: lan_unix_secs(),
            }),
        });
    }

    pub fn relay_posts(&self, fingerprint: &str, posts: Vec<SignedPost>) {
        if let Some(torrent) = &self.inner.torrent {
            let object_id = format!("feed-{fingerprint}-{}", lan_unix_secs());
            if let Ok(bytes) = serde_json::to_vec(&SwarmPostsBlob {
                posts: posts.clone(),
                updated_at: lan_unix_secs(),
            }) {
                if let Ok(magnet) = torrent.publish(&object_id, &bytes) {
                    if let Some(dht) = self.inner.dht.lock().unwrap().as_ref() {
                        if let Ok(value) = serde_json::to_vec(
                            &serde_json::json!({"magnet": magnet, "object_id": object_id}),
                        ) {
                            let _ = dht.publish("snartnet/feed", &[fingerprint], &value);
                        }
                    }
                }
            }
        }
        self.fanout_put(&TransportRequest::PutPosts {
            fingerprint: fingerprint.into(),
            blob: SwarmPostsBlob {
                posts,
                updated_at: lan_unix_secs(),
            },
        });
    }

    pub fn relay_message(&self, message: &SignedMessage) -> bool {
        if let (Some(torrent), Some(dht), Some(keypair)) = (
            &self.inner.torrent,
            self.inner.dht.lock().unwrap().clone(),
            self.inner.identity.lock().unwrap().clone(),
        ) {
            let legacy = crate::protocol::SignedEnvelope::from_signed_message(message);
            if let Ok(envelope) = crate::protocol::SignedEnvelope::sign(legacy.envelope, &keypair) {
                let _ = self.publish_mailbox_manifest(torrent, &dht, &keypair, message, &envelope);
            }
        }
        self.fanout_put(&TransportRequest::PutInbox {
            recipient_fingerprint: message.message.recipient_fingerprint.clone(),
            blob: SwarmInboxBlob {
                messages: vec![message.clone()],
                updated_at: lan_unix_secs(),
            },
        }) > 0
    }

    fn publish_mailbox_manifest(
        &self,
        torrent: &TorrentNode,
        dht: &DhtNode,
        keypair: &KeyPair,
        message: &SignedMessage,
        envelope: &crate::protocol::SignedEnvelope,
    ) -> Result<(), String> {
        let sender = &message.message.sender_fingerprint;
        let recipient = &message.message.recipient_fingerprint;
        let object_id = format!("message-{}", message.message.id);
        let magnet = torrent.publish(&object_id, &envelope.to_json()?)?;
        let now = chrono::Utc::now();
        let current = dht
            .get_for(&keypair.public_key, "snartnet/mailbox", &[sender, recipient])?
            .and_then(|value| serde_json::from_slice::<serde_json::Value>(&value).ok())
            .and_then(|value| Some((
                value.get("magnet")?.as_str()?.to_owned(),
                value.get("object_id")?.as_str()?.to_owned(),
            )));
        let current_pointer = current.clone();
        let mut manifest = if let Some((manifest_magnet, manifest_id)) = current {
            torrent.fetch(&manifest_magnet, &manifest_id).ok()
                .and_then(|bytes| serde_json::from_slice::<crate::protocol::MailboxManifest>(&bytes).ok())
                .filter(|manifest| manifest.sender == *sender && manifest.recipient == *recipient
                    && manifest.verify(&keypair.public_key, now).unwrap_or(false))
        } else { None }.unwrap_or(crate::protocol::MailboxManifest {
            v: crate::protocol::WIRE_VERSION,
            kind: crate::protocol::ObjectType::MailboxManifest,
            sender: sender.clone(), recipient: recipient.clone(), sequence: 0,
            batches: Vec::new(), previous: None, signature: String::new(),
        });
        manifest.sequence = manifest.sequence.saturating_add(1);
        if manifest.batches.len() >= 4 {
            manifest.previous = current_pointer.map(|(magnet, object_id)| crate::protocol::TorrentDescriptor {
                magnet, object_id, created_at: now,
                expires_at: Some(now + chrono::Duration::days(30)),
            });
            manifest.batches.clear();
        }
        manifest.batches.push(crate::protocol::TorrentDescriptor {
            magnet, object_id, created_at: now,
            expires_at: Some(now + chrono::Duration::days(30)),
        });
        let manifest = manifest.sign(keypair)?;
        let manifest_id = format!("mailbox-{sender}-{recipient}-{}", manifest.sequence);
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        let manifest_magnet = torrent.publish(&manifest_id, &manifest_bytes)?;
        let value = serde_json::to_vec(&serde_json::json!({"magnet": manifest_magnet, "object_id": manifest_id}))
            .map_err(|e| e.to_string())?;
        dht.publish("snartnet/mailbox", &[sender, recipient], &value)?;
        Ok(())
    }

    /// Resolve immutable mailbox batches directly from the DHT and torrent swarms.
    /// No application relay is involved; failure simply returns no distributed items.
    pub fn load_distributed_messages(
        &self,
        sender: &Contact,
        recipient_fingerprint: &str,
    ) -> Vec<SignedMessage> {
        let Some(public_key) = sender.known_public_key.as_deref() else { return Vec::new() };
        let Some(torrent) = &self.inner.torrent else { return Vec::new() };
        let Some(dht) = self.inner.dht.lock().ok().and_then(|dht| dht.clone()) else { return Vec::new() };
        let Ok(Some(value)) = dht.get_for(
            public_key,
            "snartnet/mailbox",
            &[&sender.fingerprint, recipient_fingerprint],
        ) else { return Vec::new() };
        let Ok(pointer) = serde_json::from_slice::<serde_json::Value>(&value) else { return Vec::new() };
        let Some(mut magnet) = pointer.get("magnet").and_then(|value| value.as_str()).map(str::to_owned) else { return Vec::new() };
        let Some(mut object_id) = pointer.get("object_id").and_then(|value| value.as_str()).map(str::to_owned) else { return Vec::new() };
        let mut output = Vec::new();
        for _ in 0..16 {
            let Ok(bytes) = torrent.fetch(&magnet, &object_id) else { break };
            let Ok(manifest) = serde_json::from_slice::<crate::protocol::MailboxManifest>(&bytes) else { break };
            if manifest.sender != sender.fingerprint
                || manifest.recipient != recipient_fingerprint
                || !manifest.verify(public_key, chrono::Utc::now()).unwrap_or(false)
            { break; }
            for batch in &manifest.batches {
                let Ok(message_bytes) = torrent.fetch(&batch.magnet, &batch.object_id) else { continue };
                let Ok(envelope) = crate::protocol::SignedEnvelope::from_json(&message_bytes) else { continue };
                if !envelope.verify(public_key, Some(recipient_fingerprint)).unwrap_or(false) { continue; }
                let Some(content) = envelope.envelope.body.as_str() else { continue };
                output.push(SignedMessage {
                    message: Message {
                        id: envelope.envelope.id,
                        sender_fingerprint: envelope.envelope.from,
                        recipient_fingerprint: recipient_fingerprint.to_owned(),
                        content: content.to_owned(),
                        created_at: envelope.envelope.created_at,
                        encrypted: true,
                        body_enc: envelope.envelope.body_enc,
                        nonce_b64: envelope.envelope.nonce,
                        message_type: MessageType::Direct,
                    },
                    signature: envelope.signature,
                });
            }
            let Some(previous) = manifest.previous else { break };
            if previous.magnet.is_empty() { break; }
            magnet = previous.magnet;
            object_id = previous.object_id;
        }
        output
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

    fn load_profile_for_contact(&self, contact: &Contact) -> Option<SwarmProfileBlob> {
        if let Some(profile) = self.load_profile_local(&contact.fingerprint) {
            return Some(profile);
        }
        if let (Some(torrent), Some(magnet)) = (&self.inner.torrent, contact.magnet_uri.as_ref()) {
            let object_id = format!("profile-{}", contact.fingerprint);
            if let Ok(bytes) = torrent.fetch(magnet, &object_id) {
                if let Ok(profile) = serde_json::from_slice::<SignedProfile>(&bytes) {
                    let blob = SwarmProfileBlob {
                        profile,
                        updated_at: lan_unix_secs(),
                    };
                    if self.save_profile_local(&contact.fingerprint, &blob).is_ok() {
                        return Some(blob);
                    }
                }
            }
        }
        self.load_profile(&contact.fingerprint)
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

pub fn sanitize_component(s: &str) -> String {
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
