//! Internet-wide peer discovery, change notification, and direct chat via iroh.
//!
//! `discovery.rs` only finds peers on the local subnet. This module extends discovery
//! beyond the LAN: each node opens an [`iroh`] `Endpoint` keyed on the profile's Ed25519
//! signing key (so a peer's iroh node id equals its existing SnartNet public key), uses
//! iroh's default (n0) discovery services to find other nodes by that id, and joins a
//! single well-known [`iroh_gossip`] topic shared by every SnartNet peer.
//!
//! Gossip only ever carries small control-plane payloads: presence announcements and
//! "something changed" notices. Profile and post objects still travel over the existing
//! BitTorrent/DHT transport (see `torrent.rs` / `dht.rs`).
//!
//! Chat messages are different: two peers that are both behind restrictive NATs may
//! never become reachable over plain TCP or BitTorrent. The same iroh `Endpoint` used
//! for gossip also accepts a second, dedicated ALPN ([`CHAT_ALPN`]) used to open a
//! direct QUIC connection to a peer by node id. iroh transparently attempts NAT hole
//! punching and falls back to relaying traffic through an iroh relay server when a
//! direct path cannot be established, so a chat message can reach its recipient even
//! when neither side has a port-forwarded, publicly reachable address. By default this
//! uses iroh's *staging* (test) relay infrastructure; see [`relay_mode_from_env`].
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bytes::Bytes;
use iroh::{
    endpoint::{presets, Connection},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, PublicKey, RelayMode, SecretKey,
};
use iroh_gossip::{
    api::{Event, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use n0_future::StreamExt as _;
use serde::{Deserialize, Serialize};
use snartnet_core::KeyPair;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::runtime::{Builder, Runtime};

use crate::discovery::{lan_unix_secs, DiscoveredPeer};

/// Fixed namespace all SnartNet peers gossip on to exchange presence and update notices.
const GOSSIP_NAMESPACE: &str = "snartnet-gossip-v1";

/// How often (seconds) we (re-)broadcast our own presence on the gossip topic.
const PRESENCE_INTERVAL_SECS: u64 = 30;

/// Milliseconds between shutdown-check iterations inside background loops.
const SHUTDOWN_CHECK_INTERVAL_MS: u64 = 500;

/// Seconds after last-seen before a gossip-discovered peer is considered gone.
const PEER_EXPIRY_SECS: u64 = 180;

/// ALPN identifying the direct peer-to-peer chat protocol carried over the same iroh
/// endpoint used for gossip. Distinct from [`GOSSIP_ALPN`] so the two protocols can be
/// dispatched independently by the [`Router`].
pub const CHAT_ALPN: &[u8] = b"snartnet/chat/1";

/// Largest chat envelope accepted over the direct iroh channel (1 MiB). Generous for a
/// signed text message; guards a misbehaving peer from exhausting memory.
const MAX_CHAT_MESSAGE_BYTES: usize = 1024 * 1024;

/// How long to wait for a chat connection, send, and acknowledgement before giving up.
const CHAT_SEND_TIMEOUT: Duration = Duration::from_secs(15);

/// Environment variable selecting which iroh relay infrastructure to use. One of
/// `staging` (default; iroh's test relays), `prod`/`production`, or `off`/`disabled`.
pub const ENV_RELAY_MODE: &str = "SNARTNET_IROH_RELAY";

/// Picks the iroh [`RelayMode`] for our endpoint from [`ENV_RELAY_MODE`].
///
/// Direct NAT-to-NAT connections frequently need a relay to punch through, so this
/// defaults to iroh's staging (test) relay servers rather than disabling relays. Set
/// `SNARTNET_IROH_RELAY=prod` to use n0's production relays instead, or `off` to
/// disable relaying entirely (only same-network/directly-reachable peers will connect).
pub fn relay_mode_from_env() -> RelayMode {
    match std::env::var(ENV_RELAY_MODE) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "prod" | "production" | "default" => RelayMode::Default,
            "off" | "disabled" | "none" => RelayMode::Disabled,
            _ => RelayMode::Staging,
        },
        Err(_) => RelayMode::Staging,
    }
}

fn topic_id() -> TopicId {
    TopicId::from_bytes(*blake3::hash(GOSSIP_NAMESPACE.as_bytes()).as_bytes())
}

/// Decodes a base64-encoded Ed25519 public key (as stored on `Contact`/`Profile`) into
/// the iroh node id used to dial that peer directly.
pub fn public_key_from_base64(key: &str) -> Option<PublicKey> {
    let bytes = STANDARD.decode(key).ok()?;
    let bytes: [u8; 32] = bytes.try_into().ok()?;
    PublicKey::from_bytes(&bytes).ok()
}

/// Accept-side handler for [`CHAT_ALPN`]: reads one length-bounded message per
/// connection into `inbox` and sends back a small acknowledgement.
#[derive(Debug, Clone)]
struct ChatProtocol {
    inbox: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl ProtocolHandler for ChatProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = connection.accept_bi().await?;
        let data = recv
            .read_to_end(MAX_CHAT_MESSAGE_BYTES)
            .await
            .map_err(AcceptError::from_err)?;
        self.inbox.lock().unwrap().push(data);
        send.write_all(b"OK").await.map_err(AcceptError::from_err)?;
        send.finish().map_err(AcceptError::from_err)?;
        connection.closed().await;
        Ok(())
    }
}

/// What kind of change a gossip announcement is reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateKind {
    /// Periodic "I'm still here" announcement.
    Presence,
    /// The sender published a new/updated profile.
    Profile,
    /// The sender published a new post.
    Post,
}

/// Small control-plane payload broadcast over the gossip topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GossipAnnounce {
    fingerprint: String,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    /// "ip:port" of our TCP sync server, if known, so a peer can add us directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp_addr: Option<String>,
    kind: UpdateKind,
}

/// The pieces of our own identity needed to build a [`GossipAnnounce`].
#[derive(Debug, Clone)]
pub struct GossipPresence {
    pub fingerprint: String,
    pub username: String,
    pub display_name: Option<String>,
    pub tcp_addr: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GossipStatus {
    pub active: bool,
    pub node_id: Option<String>,
    pub peer_count: usize,
    pub last_error: Option<String>,
}

/// Manages internet-wide peer discovery and lightweight update signalling via
/// iroh + iroh-gossip. Discovery state is kept in memory only, mirroring
/// `LanDiscovery`.
pub struct GossipNode {
    runtime: Arc<Runtime>,
    endpoint: Endpoint,
    gossip: Gossip,
    _router: Router,
    sender: Mutex<Option<GossipSender>>,
    peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
    active: Arc<AtomicBool>,
    status: Mutex<GossipStatus>,
    /// Raw payloads received over [`CHAT_ALPN`], awaiting pickup by `drain_chat_inbox`.
    chat_inbox: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl GossipNode {
    /// Open an iroh endpoint whose node id is derived from `keypair`'s signing key, and
    /// spawn a gossip actor accepting connections on it. Does not join any topic yet.
    pub fn open(keypair: &KeyPair) -> Result<Self, String> {
        let secret = STANDARD
            .decode(&keypair.secret_key)
            .map_err(|e| format!("invalid signing key: {e}"))?;
        let bytes: [u8; 32] = secret
            .try_into()
            .map_err(|_| "invalid signing key length".to_string())?;
        let secret_key = SecretKey::from_bytes(&bytes);
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .map_err(|e| format!("gossip runtime failed: {e}"))?,
        );
        let (endpoint, router, gossip, chat_inbox) = runtime.block_on(async {
            let endpoint = Endpoint::builder(presets::N0)
                .secret_key(secret_key)
                .relay_mode(relay_mode_from_env())
                .alpns(vec![GOSSIP_ALPN.to_vec(), CHAT_ALPN.to_vec()])
                .bind()
                .await
                .map_err(|e| format!("iroh endpoint bind failed: {e}"))?;
            let gossip = Gossip::builder().spawn(endpoint.clone());
            let chat_inbox = Arc::new(Mutex::new(Vec::new()));
            let router = Router::builder(endpoint.clone())
                .accept(GOSSIP_ALPN, gossip.clone())
                .accept(
                    CHAT_ALPN,
                    ChatProtocol {
                        inbox: chat_inbox.clone(),
                    },
                )
                .spawn();
            Ok::<_, String>((endpoint, router, gossip, chat_inbox))
        })?;
        let node_id = endpoint.id().to_string();
        Ok(Self {
            runtime,
            endpoint,
            gossip,
            _router: router,
            sender: Mutex::new(None),
            peers: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(AtomicBool::new(false)),
            status: Mutex::new(GossipStatus {
                active: false,
                node_id: Some(node_id),
                peer_count: 0,
                last_error: None,
            }),
            chat_inbox,
        })
    }

    /// Our iroh node id, hex-encoded. Equal to the profile's Ed25519 public key.
    pub fn node_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// Join the shared SnartNet gossip topic, announce our presence, and start
    /// broadcasting it periodically. `bootstrap` is a list of base64-encoded Ed25519
    /// public keys of already-known/verified contacts, used as initial gossip peers.
    ///
    /// Restarting replaces any previous subscription; each generation has its own
    /// stop token so old background tasks cannot outlive a restart.
    pub fn start(self: &Arc<Self>, presence: GossipPresence, bootstrap: Vec<String>) {
        self.stop();
        self.active.store(true, Ordering::Relaxed);
        if let Ok(mut status) = self.status.lock() {
            status.active = true;
            status.last_error = None;
        }

        let bootstrap_ids: Vec<PublicKey> = bootstrap
            .iter()
            .filter_map(|key| public_key_from_base64(key))
            .collect();

        let this = self.clone();
        self.runtime.spawn(async move {
            let topic = match this.gossip.subscribe(topic_id(), bootstrap_ids).await {
                Ok(topic) => topic,
                Err(error) => {
                    if let Ok(mut status) = this.status.lock() {
                        status.last_error = Some(format!("gossip subscribe failed: {error}"));
                    }
                    return;
                }
            };
            let (sender, mut receiver) = topic.split();
            *this.sender.lock().unwrap() = Some(sender.clone());

            let recv_this = this.clone();
            let recv_presence = presence.clone();
            let receiver_task = async move {
                while recv_this.active.load(Ordering::Relaxed) {
                    let next = tokio::time::timeout(
                        Duration::from_millis(SHUTDOWN_CHECK_INTERVAL_MS),
                        receiver.next(),
                    )
                    .await;
                    let Ok(Some(Ok(event))) = next else {
                        continue;
                    };
                    if let Event::Received(message) = event {
                        recv_this.ingest(&message.content, &recv_presence.fingerprint);
                    }
                }
            };

            let send_this = this.clone();
            let sender_task = async move {
                while send_this.active.load(Ordering::Relaxed) {
                    send_this
                        .broadcast_now(&sender, UpdateKind::Presence, &presence)
                        .await;
                    let checks = PRESENCE_INTERVAL_SECS * 1000 / SHUTDOWN_CHECK_INTERVAL_MS;
                    for _ in 0..checks {
                        if !send_this.active.load(Ordering::Relaxed) {
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(SHUTDOWN_CHECK_INTERVAL_MS))
                            .await;
                    }
                }
            };

            tokio::join!(receiver_task, sender_task);
        });
    }

    async fn broadcast_now(
        &self,
        sender: &GossipSender,
        kind: UpdateKind,
        presence: &GossipPresence,
    ) {
        let payload = GossipAnnounce {
            fingerprint: presence.fingerprint.clone(),
            username: presence.username.clone(),
            display_name: presence.display_name.clone(),
            tcp_addr: presence.tcp_addr.clone(),
            kind,
        };
        if let Ok(bytes) = serde_json::to_vec(&payload) {
            let _ = sender.broadcast(Bytes::from(bytes)).await;
        }
    }

    fn ingest(&self, content: &[u8], own_fingerprint: &str) {
        let Ok(msg) = serde_json::from_slice::<GossipAnnounce>(content) else {
            return;
        };
        if msg.fingerprint == own_fingerprint {
            return;
        }
        let now = lan_unix_secs();
        let mut guard = self.peers.lock().unwrap();
        if let Some(existing) = guard.iter_mut().find(|p| p.fingerprint == msg.fingerprint) {
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
        if let Ok(mut status) = self.status.lock() {
            status.peer_count = guard.len();
        }
    }

    /// Broadcast that our profile or a post changed, so peers can react promptly instead
    /// of waiting for their next poll. Best-effort: silently does nothing if we have not
    /// (yet) joined the topic.
    pub fn announce_update(&self, kind: UpdateKind, presence: GossipPresence) {
        let Some(sender) = self.sender.lock().unwrap().clone() else {
            return;
        };
        self.runtime.spawn(async move {
            let payload = GossipAnnounce {
                fingerprint: presence.fingerprint,
                username: presence.username,
                display_name: presence.display_name,
                tcp_addr: presence.tcp_addr,
                kind,
            };
            if let Ok(bytes) = serde_json::to_vec(&payload) {
                let _ = sender.broadcast(Bytes::from(bytes)).await;
            }
        });
    }

    /// Stop broadcasting/listening on the gossip topic. Already-discovered peers are cleared.
    pub fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        *self.sender.lock().unwrap() = None;
        self.peers.lock().unwrap().clear();
        if let Ok(mut status) = self.status.lock() {
            status.active = false;
            status.peer_count = 0;
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Snapshot of currently-visible gossip peers, evicting stale entries first.
    pub fn get_discovered(&self) -> Vec<DiscoveredPeer> {
        let mut guard = self.peers.lock().unwrap();
        guard.retain(|p| is_peer_fresh(p.last_seen));
        guard.clone()
    }

    pub fn status(&self) -> GossipStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    /// Dial `target` directly over iroh (using NAT hole punching, or the configured
    /// relay as a fallback) and hand it `payload` on a fresh bidirectional stream.
    ///
    /// This does not require the gossip topic to be joined; it works as soon as the
    /// endpoint is open. Blocks the calling thread until the peer acknowledges receipt
    /// or [`CHAT_SEND_TIMEOUT`] elapses. Returns `true` only once the peer has
    /// acknowledged the message.
    pub fn send_chat(&self, target: PublicKey, payload: Vec<u8>) -> bool {
        let endpoint = self.endpoint.clone();
        self.runtime.block_on(async move {
            tokio::time::timeout(CHAT_SEND_TIMEOUT, async move {
                let connection = endpoint
                    .connect(target, CHAT_ALPN)
                    .await
                    .map_err(|e| e.to_string())?;
                let (mut send, mut recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
                send.write_all(&payload).await.map_err(|e| e.to_string())?;
                send.finish().map_err(|e| e.to_string())?;
                let ack = recv.read_to_end(64).await.map_err(|e| e.to_string())?;
                connection.close(0u32.into(), b"done");
                Ok::<_, String>(!ack.is_empty())
            })
            .await
            .unwrap_or(Ok(false))
            .unwrap_or(false)
        })
    }

    /// Take and clear all chat payloads received over [`CHAT_ALPN`] since the last call.
    pub fn drain_chat_inbox(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.chat_inbox.lock().unwrap())
    }
}

#[inline]
fn is_peer_fresh(last_seen: u64) -> bool {
    lan_unix_secs().saturating_sub(last_seen) < PEER_EXPIRY_SECS
}

impl Drop for GossipNode {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_matches_the_profile_public_key() {
        let keypair = KeyPair::generate().unwrap();
        let node = GossipNode::open(&keypair).unwrap();
        let public_key_bytes = STANDARD.decode(&keypair.public_key).unwrap();
        let expected = public_key_bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(node.node_id(), expected);
    }

    #[test]
    fn opening_the_same_key_twice_yields_a_stable_node_id() {
        let keypair = KeyPair::generate().unwrap();
        let a = GossipNode::open(&keypair).unwrap();
        let b = GossipNode::open(&keypair).unwrap();
        assert_eq!(a.node_id(), b.node_id());
    }

    #[test]
    fn topic_id_is_deterministic() {
        assert_eq!(topic_id(), topic_id());
    }

    #[test]
    fn public_key_from_base64_matches_node_id() {
        let keypair = KeyPair::generate().unwrap();
        let node = GossipNode::open(&keypair).unwrap();
        let decoded = public_key_from_base64(&keypair.public_key).expect("valid key");
        assert_eq!(decoded.to_string(), node.node_id());
    }

    #[test]
    fn public_key_from_base64_rejects_garbage() {
        assert!(public_key_from_base64("not-base64!!").is_none());
        assert!(public_key_from_base64("").is_none());
    }

    #[test]
    fn relay_mode_env_defaults_to_staging_test_relays() {
        // SAFETY: tests run single-threaded per-process for env-var mutation here isn't
        // guaranteed, but this only reads/restores a var private to this test's checks.
        let previous = std::env::var(ENV_RELAY_MODE).ok();
        std::env::remove_var(ENV_RELAY_MODE);
        assert_eq!(relay_mode_from_env(), RelayMode::Staging);

        std::env::set_var(ENV_RELAY_MODE, "prod");
        assert_eq!(relay_mode_from_env(), RelayMode::Default);

        std::env::set_var(ENV_RELAY_MODE, "off");
        assert_eq!(relay_mode_from_env(), RelayMode::Disabled);

        std::env::set_var(ENV_RELAY_MODE, "staging");
        assert_eq!(relay_mode_from_env(), RelayMode::Staging);

        match previous {
            Some(value) => std::env::set_var(ENV_RELAY_MODE, value),
            None => std::env::remove_var(ENV_RELAY_MODE),
        }
    }

    /// End-to-end proof that two independent nodes can exchange a chat message purely
    /// over iroh (no shared process state, no LAN broadcast). Requires real network
    /// egress to iroh's relay/discovery infrastructure, so it's ignored by default;
    /// run explicitly with `cargo test -p snartnet-client -- --ignored`.
    #[test]
    #[ignore = "requires network access to iroh's staging relay/discovery infrastructure"]
    fn two_nodes_exchange_a_direct_chat_message_via_iroh() {
        let alice = GossipNode::open(&KeyPair::generate().unwrap()).unwrap();
        let bob = GossipNode::open(&KeyPair::generate().unwrap()).unwrap();
        let bob_id: PublicKey = bob.node_id().parse().expect("valid node id");

        let delivered = alice.send_chat(bob_id, b"hello from alice".to_vec());
        assert!(delivered, "message should be acknowledged by the peer");

        let inbox = bob.drain_chat_inbox();
        assert_eq!(inbox, vec![b"hello from alice".to_vec()]);
    }
}
