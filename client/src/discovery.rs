//! Optional local-network presence. Each start has an independent stop token.
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

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
    socket: Option<Arc<std::net::UdpSocket>>,
}

impl LanDiscovery {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(AtomicBool::new(false)),
            socket: None,
        }
    }

    /// Start broadcasting `announce` and listening for other peers.
    ///
    /// Returns `true` if both sockets were successfully created.  Returns
    /// `false` if the listener socket could not be bound (firewall, port
    /// already in use, etc.) – callers should degrade gracefully.
    pub fn start(&mut self, announce: LanAnnounce) -> bool {
        self.stop();
        // Each generation has its own stop token; restarting cannot revive old listeners.
        self.active = Arc::new(AtomicBool::new(false));
        // Try to bind the listener socket first; bail if this fails so we
        // don't start a sender without a corresponding receiver.
        let listener = match &self.socket {
            Some(socket) => socket.clone(),
            None => {
                let Ok(socket) = std::net::UdpSocket::bind(format!("0.0.0.0:{LAN_DISCOVERY_PORT}"))
                else {
                    return false;
                };
                let socket = Arc::new(socket);
                self.socket = Some(socket.clone());
                socket
            }
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
                // Timeouts give the stop token a chance to end this generation.
                if let Ok((len, _src)) = listener.recv_from(&mut buf) {
                    if !active_listener.load(Ordering::Relaxed) {
                        return;
                    }
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

pub fn lan_unix_secs() -> u64 {
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

impl Drop for LanDiscovery {
    fn drop(&mut self) {
        self.stop();
    }
}
