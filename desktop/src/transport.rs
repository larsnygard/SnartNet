use serde::{Deserialize, Serialize};
use snartnet_core::{SignedMessage, SignedPost, SignedProfile};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
        blob: SwarmProfileBlob,
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
    Profile { blob: Option<SwarmProfileBlob> },
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
    peers: Vec<SocketAddr>,
}

impl TcpSwarmTransport {
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
                peers,
            }),
        })
    }

    pub fn start_server(&self) {
        let this = self.clone();
        std::thread::spawn(move || {
            let listener = match TcpListener::bind(this.inner.bind_addr) {
                Ok(v) => v,
                Err(_) => return,
            };
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let req: Result<TransportRequest, String> = (|| {
                    let mut buf = Vec::new();
                    stream
                        .read_to_end(&mut buf)
                        .map_err(|e| format!("read failed: {e}"))?;
                    serde_json::from_slice::<TransportRequest>(&buf)
                        .map_err(|e| format!("parse failed: {e}"))
                })();

                let resp = match req {
                    Ok(r) => this.handle_request(r),
                    Err(e) => TransportResponse::Err { message: e },
                };

                let payload = serde_json::to_vec(&resp).unwrap_or_else(|_| {
                    b"{\"kind\":\"err\",\"message\":\"serialization failed\"}".to_vec()
                });
                let _ = stream.write_all(&payload);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
            }
        });
    }

    fn handle_request(&self, req: TransportRequest) -> TransportResponse {
        match req {
            TransportRequest::GetProfile { fingerprint } => TransportResponse::Profile {
                blob: self.load_profile_local(&fingerprint),
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
        stream.read_to_end(&mut out).ok()?;
        serde_json::from_slice::<TransportResponse>(&out).ok()
    }

    fn fanout_put(&self, req: &TransportRequest) {
        for peer in &self.inner.peers {
            let _ = self.request_peer(*peer, req);
        }
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
        save_json_file(&self.profile_path(fingerprint), blob)
    }

    fn load_posts_local(&self, fingerprint: &str) -> Option<SwarmPostsBlob> {
        load_json_file(&self.posts_path(fingerprint)).ok().flatten()
    }

    fn save_posts_local(&self, fingerprint: &str, blob: &SwarmPostsBlob) -> Result<(), String> {
        save_json_file(&self.posts_path(fingerprint), blob)
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
        save_json_file(&self.inbox_path(recipient_fingerprint), blob)
    }
}

impl NetworkTransport for TcpSwarmTransport {
    fn load_profile(&self, fingerprint: &str) -> Option<SwarmProfileBlob> {
        if let Some(v) = self.load_profile_local(fingerprint) {
            return Some(v);
        }

        for peer in &self.inner.peers {
            let req = TransportRequest::GetProfile {
                fingerprint: fingerprint.to_string(),
            };
            if let Some(TransportResponse::Profile { blob: Some(blob) }) =
                self.request_peer(*peer, &req)
            {
                let _ = self.save_profile_local(fingerprint, &blob);
                return Some(blob);
            }
        }
        None
    }

    fn save_profile(&self, fingerprint: &str, blob: &SwarmProfileBlob) -> Result<(), String> {
        self.save_profile_local(fingerprint, blob)?;
        let req = TransportRequest::PutProfile {
            fingerprint: fingerprint.to_string(),
            blob: blob.clone(),
        };
        self.fanout_put(&req);
        Ok(())
    }

    fn load_posts(&self, fingerprint: &str) -> Option<SwarmPostsBlob> {
        if let Some(v) = self.load_posts_local(fingerprint) {
            return Some(v);
        }

        for peer in &self.inner.peers {
            let req = TransportRequest::GetPosts {
                fingerprint: fingerprint.to_string(),
            };
            if let Some(TransportResponse::Posts { blob: Some(blob) }) =
                self.request_peer(*peer, &req)
            {
                let _ = self.save_posts_local(fingerprint, &blob);
                return Some(blob);
            }
        }
        None
    }

    fn save_posts(&self, fingerprint: &str, blob: &SwarmPostsBlob) -> Result<(), String> {
        self.save_posts_local(fingerprint, blob)?;
        let req = TransportRequest::PutPosts {
            fingerprint: fingerprint.to_string(),
            blob: blob.clone(),
        };
        self.fanout_put(&req);
        Ok(())
    }

    fn load_inbox(&self, recipient_fingerprint: &str) -> Option<SwarmInboxBlob> {
        let mut local = self
            .load_inbox_local(recipient_fingerprint)
            .unwrap_or_default();
        let mut changed = false;

        for peer in &self.inner.peers {
            let req = TransportRequest::GetInbox {
                recipient_fingerprint: recipient_fingerprint.to_string(),
            };
            if let Some(TransportResponse::Inbox {
                blob: Some(mut remote),
            }) = self.request_peer(*peer, &req)
            {
                let before = local.messages.len();
                local.messages.append(&mut remote.messages);
                dedupe_inbox(&mut local);
                if local.messages.len() != before {
                    changed = true;
                }
            }
        }

        if changed {
            let _ = self.save_inbox_local(recipient_fingerprint, &local);
        }

        Some(local)
    }

    fn save_inbox(&self, recipient_fingerprint: &str, blob: &SwarmInboxBlob) -> Result<(), String> {
        let mut cloned = blob.clone();
        dedupe_inbox(&mut cloned);

        self.save_inbox_local(recipient_fingerprint, &cloned)?;
        let req = TransportRequest::PutInbox {
            recipient_fingerprint: recipient_fingerprint.to_string(),
            blob: cloned,
        };
        self.fanout_put(&req);
        Ok(())
    }
}

fn swarm_root_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory".to_string())?;
    let root = PathBuf::from(home).join(".snartnet").join("swarm");
    std::fs::create_dir_all(&root).map_err(|e| format!("failed to create swarm dir: {e}"))?;
    Ok(root)
}

fn sanitize_component(s: &str) -> String {
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
    std::fs::write(path, text).map_err(|e| format!("write failed: {e}"))
}

pub fn dedupe_inbox(inbox: &mut SwarmInboxBlob) {
    let mut seen = std::collections::HashSet::new();
    inbox.messages.retain(|m| seen.insert(m.message.id.clone()));
}

// ---------------------------------------------------------------------------
// LAN peer discovery (UDP broadcast)
// ---------------------------------------------------------------------------

/// UDP port used for LAN presence announcements.
pub const LAN_DISCOVERY_PORT: u16 = 47471;

/// How often (seconds) the local node re-broadcasts its presence.
const BROADCAST_INTERVAL_SECS: u64 = 30;

/// Milliseconds between shutdown-check iterations inside the sender sleep loop.
const SHUTDOWN_CHECK_INTERVAL_MS: u64 = 500;

/// Seconds after last-seen before a peer is considered gone.
const PEER_EXPIRY_SECS: u64 = 120;

/// The payload broadcast over UDP so nearby SnartNet peers can discover us.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanAnnounce {
    pub fingerprint: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// "ip:port" of our TCP sync server so a peer can add us directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_addr: Option<String>,
}

/// A peer discovered on the local network via UDP broadcast.
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub fingerprint: String,
    pub username: String,
    pub display_name: Option<String>,
    /// TCP sync address advertised by the peer, if provided.
    pub tcp_addr: Option<String>,
    /// Unix-epoch seconds of the most recent announcement.
    pub last_seen: u64,
}

/// Manages LAN peer discovery: broadcasts our own presence and listens for
/// announcements from nearby SnartNet peers.
///
/// Discovery state is kept in memory only; it is intentionally separate from
/// the durable contact/trust data managed by `FileStorage`.
pub struct LanDiscovery {
    peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
    active: Arc<AtomicBool>,
}

impl LanDiscovery {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start broadcasting `announce` and listening for other peers.
    ///
    /// Returns `true` if both sockets were successfully created.  Returns
    /// `false` if the listener socket could not be bound (firewall, port
    /// already in use, etc.) – callers should degrade gracefully.
    pub fn start(&self, announce: LanAnnounce) -> bool {
        // Try to bind the listener socket first; bail if this fails so we
        // don't start a sender without a corresponding receiver.
        let listener = match std::net::UdpSocket::bind(format!("0.0.0.0:{LAN_DISCOVERY_PORT}")) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let _ = listener.set_read_timeout(Some(Duration::from_millis(500)));

        self.active.store(true, Ordering::Relaxed);
        let active_listener = self.active.clone();
        let active_sender = self.active.clone();
        let peers_listener = self.peers.clone();
        let own_fp = announce.fingerprint.clone();

        // Listener thread – receives UDP datagrams from peers.
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while active_listener.load(Ordering::Relaxed) {
                match listener.recv_from(&mut buf) {
                    Ok((len, _src)) => {
                        let Ok(msg) = serde_json::from_slice::<LanAnnounce>(&buf[..len]) else {
                            continue;
                        };
                        // Skip our own broadcasts.
                        if msg.fingerprint == own_fp {
                            continue;
                        }
                        let now = lan_unix_secs();
                        let mut guard = peers_listener.lock().unwrap();
                        if let Some(existing) =
                            guard.iter_mut().find(|p| p.fingerprint == msg.fingerprint)
                        {
                            existing.last_seen = now;
                            existing.username.clone_from(&msg.username);
                            existing.tcp_addr.clone_from(&msg.tcp_addr);
                            existing.display_name.clone_from(&msg.display_name);
                        } else {
                            guard.push(DiscoveredPeer {
                                fingerprint: msg.fingerprint,
                                username: msg.username,
                                display_name: msg.display_name,
                                tcp_addr: msg.tcp_addr,
                                last_seen: now,
                            });
                        }
                        guard.retain(|p| is_peer_fresh(p.last_seen));
                    }
                    Err(_) => {} // read-timeout or error; just loop
                }
            }
        });

        // Sender thread – periodically broadcasts our own presence.
        std::thread::spawn(move || {
            let sender = match std::net::UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(_) => return,
            };
            let _ = sender.set_broadcast(true);
            let broadcast_addr: SocketAddr = format!("255.255.255.255:{LAN_DISCOVERY_PORT}")
                .parse()
                .unwrap();
            while active_sender.load(Ordering::Relaxed) {
                if let Ok(payload) = serde_json::to_vec(&announce) {
                    let _ = sender.send_to(&payload, broadcast_addr);
                }
                // Sleep in short increments so the thread can exit promptly.
                let checks = BROADCAST_INTERVAL_SECS * 1000 / SHUTDOWN_CHECK_INTERVAL_MS;
                for _ in 0..checks {
                    if !active_sender.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(SHUTDOWN_CHECK_INTERVAL_MS));
                }
            }
        });

        true
    }

    /// Stop broadcasting and listening.  Any already-discovered peers are cleared.
    pub fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        self.peers.lock().unwrap().clear();
    }

    /// Whether discovery threads are currently running.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Return a snapshot of currently-visible peers, evicting stale entries first.
    pub fn get_discovered(&self) -> Vec<DiscoveredPeer> {
        let mut guard = self.peers.lock().unwrap();
        guard.retain(|p| is_peer_fresh(p.last_seen));
        guard.clone()
    }
}

impl Default for LanDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns `true` if a peer with the given `last_seen` timestamp is still
/// within the liveness window (i.e. has not yet expired).
#[inline]
fn is_peer_fresh(last_seen: u64) -> bool {
    lan_unix_secs().saturating_sub(last_seen) < PEER_EXPIRY_SECS
}

fn lan_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Best-effort attempt to find this host's primary LAN IP address.
///
/// Opens a UDP socket, "connects" it to a public address (no packets are
/// actually sent), then reads the local address the OS assigned.  Returns
/// `None` on any error so callers can fall back gracefully.
pub fn local_lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}
