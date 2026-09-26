//! Authenticated, contact-scoped peer protocol over iroh (ADR 0003, M6.3/M6.4).
//!
//! Every message on [`PEER_ALPN`] is a length-prefixed JSON frame. A connection is only
//! useful after both sides have exchanged a [`Frame::Hello`]/[`Frame::HelloAck`] pair
//! carrying profile-signed [`DeviceCertificate`]s, so an application frame is never
//! processed before the sender's identity, capabilities, and certificate window are known.
//!
//! There is no global topic and no broadcast: a peer is either a contact we hold a policy
//! for (its profile key plus newest accepted certificate), or the connection is closed with
//! [`CLOSE_UNKNOWN_CONTACT`]. This replaces the previous unsigned gossip topic, where any
//! node could announce anything to everyone.
//!
//! Address discovery (M6.5) is two-layered:
//!
//! * iroh's n0 DNS/Pkarr services publish and resolve our device endpoint id, so a contact
//!   who knows the profile can find the device across the internet.
//! * the existing BEP-44 DHT carries a profile-key-signed device descriptor as a fallback
//!   for peers whose DNS path is unavailable. See [`DeviceDescriptor`].
use crate::{
    device::{CertificateError, DeviceCertificate, DeviceKey, PinnedCertificate, CAPABILITY_PEER},
    discovery::lan_unix_secs,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use iroh::{
    address_lookup::{DnsAddressLookup, PkarrPublisher, PkarrResolver},
    endpoint::{presets, Connection, ReadExactError, RecvStream, SendStream, VarInt},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr, EndpointId, PublicKey, RelayMode, TransportAddr,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::runtime::{Builder, Runtime};

/// ALPN identifying the authenticated peer protocol. Replaces `snartnet/chat/1`.
pub const PEER_ALPN: &[u8] = b"snartnet/peer/1";

/// Largest accepted frame (1 MiB). A signed text message is orders of magnitude smaller;
/// the bound keeps a hostile length prefix from allocating unbounded memory.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Most authenticated peers tracked at once. Bounds the snapshot and the dial loop.
pub const MAX_PEERS: usize = 64;

/// Bytes in a frame's big-endian length prefix.
pub const FRAME_HEADER_BYTES: usize = 4;

/// How long a handshake (connect plus certificate exchange) may take.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long to wait for a peer to acknowledge the frames we sent.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(15);

/// How long an accept handler keeps a connection open after acknowledging.
///
/// Iroh drops a connection as soon as the accept handler returns, and the acknowledgement
/// is still in flight at that point, so the handler waits for the sender to hang up first.
/// The sender closes as soon as it has the ack, so this bound is only reached by a peer
/// that goes silent after being answered.
const ACK_LINGER_TIMEOUT: Duration = Duration::from_secs(15);

/// How often (seconds) we re-announce our presence to known contact targets.
const PRESENCE_INTERVAL_SECS: u64 = 30;

/// Milliseconds between shutdown-check iterations inside background loops.
const SHUTDOWN_CHECK_INTERVAL_MS: u64 = 500;

/// Seconds after last-seen before an authenticated peer is considered gone.
const PEER_EXPIRY_SECS: u64 = 180;

/// QUIC application close code: the peer broke the frame protocol.
pub const CLOSE_PROTOCOL_ERROR: u32 = 1;

/// QUIC application close code: the certificate names someone who is not a contact.
pub const CLOSE_UNKNOWN_CONTACT: u32 = 2;

/// QUIC application close code: the certificate failed validation.
pub const CLOSE_BAD_CERTIFICATE: u32 = 3;

/// QUIC application close code: the certificate is older than the one already accepted.
pub const CLOSE_STALE_CERTIFICATE: u32 = 4;

/// Environment variable selecting how device endpoint ids are published and resolved.
/// `dns` (default) uses iroh's n0 DNS/Pkarr services; `off` disables address lookups,
/// which is what offline tests and air-gapped deployments use.
pub const ENV_DISCOVERY_MODE: &str = "SNARTNET_IROH_DISCOVERY";

/// Environment variable selecting which iroh relay infrastructure to use. One of
/// `staging` (default; iroh's test relays), `prod`/`production`, or `off`/`disabled`.
pub const ENV_RELAY_MODE: &str = "SNARTNET_IROH_RELAY";

/// What kind of change a peer notice is reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateKind {
    /// Periodic "I'm still here" notice.
    Presence,
    /// The sender published a new/updated profile.
    Profile,
    /// The sender published a new post.
    Post,
}

/// How device addresses are published and resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryMode {
    /// Publish to and resolve from iroh's n0 DNS/Pkarr services.
    DnsPkarr,
    /// No address lookup at all: only directly-addressed peers connect.
    Off,
}

/// Endpoint configuration for [`PeerNode`].
#[derive(Debug, Clone)]
pub struct PeerOptions {
    /// Address discovery strategy.
    pub discovery: DiscoveryMode,
    /// Relay strategy used to reach peers behind NAT.
    pub relay: RelayMode,
}

impl PeerOptions {
    /// Options for a real deployment, read from the environment.
    pub fn from_env() -> Self {
        Self {
            discovery: discovery_mode_from_env(),
            relay: relay_mode_from_env(),
        }
    }

    /// Options for tests and air-gapped runs: no discovery, no relays, direct addresses only.
    pub fn local() -> Self {
        Self {
            discovery: DiscoveryMode::Off,
            relay: RelayMode::Disabled,
        }
    }
}

/// Reads [`ENV_DISCOVERY_MODE`]. Anything but an explicit `off` keeps DNS/Pkarr on,
/// because a device that cannot be resolved cannot be reached.
pub fn discovery_mode_from_env() -> DiscoveryMode {
    match std::env::var(ENV_DISCOVERY_MODE) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "none" => DiscoveryMode::Off,
            _ => DiscoveryMode::DnsPkarr,
        },
        Err(_) => DiscoveryMode::DnsPkarr,
    }
}

/// Picks the iroh [`RelayMode`] from [`ENV_RELAY_MODE`]. Direct NAT-to-NAT connections
/// frequently need a relay to punch through, so this defaults to iroh's staging (test)
/// relay servers rather than disabling relays.
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

/// One message on [`PEER_ALPN`], serialized as internally tagged JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Frame {
    /// First frame on a stream: our device certificate plus a fresh connection nonce.
    Hello {
        certificate: DeviceCertificate,
        nonce: String,
    },
    /// Answer to [`Frame::Hello`], carrying our certificate and echoing their nonce.
    HelloAck {
        certificate: DeviceCertificate,
        echo: String,
    },
    /// A signed object (message, post, or profile) for the peer's indexed store.
    Object { object: Value },
    /// "Something changed" signal, replacing the old gossip announcement.
    Notice {
        kind: UpdateKind,
        fingerprint: String,
    },
    /// Everything received before this frame was persisted; `frames` says how many.
    Ack { frames: u32 },
    /// Clean shutdown of a conversation.
    Goodbye,
}

/// Why a frame or a peer conversation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerError {
    /// A frame declared a payload larger than [`MAX_FRAME_BYTES`].
    FrameTooLarge { size: usize, max: usize },
    /// A frame declared a zero-length payload.
    EmptyFrame,
    /// The payload was not a well-formed frame.
    Malformed(String),
    /// The stream ended in the middle of a frame.
    Truncated,
    /// The stream ended before any frame arrived.
    StreamClosed,
    /// A handshake was expected but a different frame arrived first.
    ExpectedHello,
    /// The handshake failed: certificate, echo, or ordering problem.
    Handshake(String),
    /// A certificate was refused. Carries the rule that refused it.
    Certificate(CertificateError),
    /// The peer is not one of our contacts.
    NotAContact(String),
    /// The peer acknowledged a different connection than ours.
    NonceMismatch,
    /// The peer did not answer in time.
    Timeout,
    /// The transport failed.
    Transport(String),
}

impl fmt::Display for PeerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooLarge { size, max } => {
                write!(f, "frame of {size} bytes exceeds the {max} byte limit")
            }
            Self::EmptyFrame => write!(f, "empty frame"),
            Self::Malformed(reason) => write!(f, "malformed frame: {reason}"),
            Self::Truncated => write!(f, "stream ended inside a frame"),
            Self::StreamClosed => write!(f, "stream closed before the handshake"),
            Self::ExpectedHello => write!(f, "the first frame was not a hello"),
            Self::Handshake(reason) => write!(f, "handshake failed: {reason}"),
            Self::Certificate(error) => write!(f, "{error}"),
            Self::NotAContact(fingerprint) => write!(f, "{fingerprint} is not a known contact"),
            Self::NonceMismatch => write!(f, "the peer echoed a different connection nonce"),
            Self::Timeout => write!(f, "the peer did not answer in time"),
            Self::Transport(reason) => write!(f, "peer transport failed: {reason}"),
        }
    }
}

impl std::error::Error for PeerError {}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Validate a frame's length prefix and return the payload length it promises.
///
/// Split out from the async reader so the size rules are testable without a socket: a
/// zero-length frame is a protocol error, and a length above `max` is refused before any
/// memory is allocated for it.
pub fn validated_payload_len(
    header: [u8; FRAME_HEADER_BYTES],
    max: usize,
) -> Result<usize, PeerError> {
    let size = u32::from_be_bytes(header) as usize;
    if size == 0 {
        return Err(PeerError::EmptyFrame);
    }
    if size > max {
        return Err(PeerError::FrameTooLarge { size, max });
    }
    Ok(size)
}

/// Serialize a frame as its length prefix followed by JSON.
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, PeerError> {
    let payload = serde_json::to_vec(frame)
        .map_err(|e| PeerError::Malformed(format!("frame serialization failed: {e}")))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(PeerError::FrameTooLarge {
            size: payload.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let mut out = Vec::with_capacity(FRAME_HEADER_BYTES + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Decode a frame payload that already passed [`validated_payload_len`].
pub fn decode_frame(payload: &[u8]) -> Result<Frame, PeerError> {
    if payload.is_empty() {
        return Err(PeerError::EmptyFrame);
    }
    serde_json::from_slice(payload).map_err(|e| PeerError::Malformed(e.to_string()))
}

/// Read exactly one frame. `Ok(None)` means the peer finished the stream cleanly.
///
/// A stream that ends exactly on a frame boundary is how a peer says "that was everything";
/// ending part-way through a frame is a truncation. The accept side distinguishes the two,
/// because it only acknowledges a delivery once it has read to the end of the conversation.
pub async fn read_frame(recv: &mut RecvStream, max: usize) -> Result<Option<Frame>, PeerError> {
    let mut header = [0u8; FRAME_HEADER_BYTES];
    match recv.read_exact(&mut header).await {
        Ok(()) => {}
        Err(ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(ReadExactError::FinishedEarly(_)) => return Err(PeerError::Truncated),
        Err(error) => return Err(PeerError::Transport(error.to_string())),
    }
    let size = validated_payload_len(header, max)?;
    let mut payload = vec![0u8; size];
    recv.read_exact(&mut payload)
        .await
        .map_err(|_| PeerError::Truncated)?;
    decode_frame(&payload).map(Some)
}

/// Write one frame into the stream.
pub async fn write_frame(send: &mut SendStream, frame: &Frame) -> Result<(), PeerError> {
    let bytes = encode_frame(frame)?;
    send.write_all(&bytes)
        .await
        .map_err(|e| PeerError::Transport(e.to_string()))
}

/// A 16-byte random nonce, base64-encoded, binding a handshake to one connection.
pub fn connection_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

// ---------------------------------------------------------------------------
// Contact policy and peer bookkeeping
// ---------------------------------------------------------------------------

/// What we know about one contact before it may talk to us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactPolicy {
    /// Fingerprint of the contact.
    pub fingerprint: String,
    /// Base64 profile key from the contact's verified profile.
    pub public_key: String,
    /// Endpoint id of the newest certificate accepted from this contact.
    pub pinned_endpoint: Option<String>,
    /// `issued_at` of that certificate, so a replayed older one is refused.
    pub pinned_issued_at: Option<u64>,
}

impl ContactPolicy {
    /// Start a policy from a verified contact. Pins are learned on the first handshake.
    pub fn new(fingerprint: &str, public_key: &str) -> Self {
        Self {
            fingerprint: fingerprint.to_string(),
            public_key: public_key.to_string(),
            pinned_endpoint: None,
            pinned_issued_at: None,
        }
    }

    /// The pin to enforce, if this contact authenticated with us before.
    pub fn pin(&self) -> Option<PinnedCertificate> {
        match (&self.pinned_endpoint, self.pinned_issued_at) {
            (Some(endpoint_id), Some(issued_at)) => Some(PinnedCertificate {
                profile: self.fingerprint.clone(),
                endpoint_id: endpoint_id.clone(),
                issued_at,
                expires_at: u64::MAX,
            }),
            _ => None,
        }
    }
}

/// A contact device to dial: its profile, its newest known endpoint id, and any direct
/// addresses learned from the DHT or the LAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerTarget {
    /// Fingerprint of the contact this device belongs to.
    pub fingerprint: String,
    /// Hex endpoint id of the contact's device.
    pub endpoint_id: String,
    /// Direct socket addresses, when known.
    pub addrs: Vec<SocketAddr>,
}

impl PeerTarget {
    /// The endpoint id as a dialable [`PublicKey`].
    pub fn endpoint_public_key(&self) -> Result<PublicKey, String> {
        self.endpoint_id
            .parse::<PublicKey>()
            .map_err(|e| format!("invalid endpoint id: {e}"))
    }

    /// The iroh address to dial: the endpoint id plus every direct address we know.
    pub fn endpoint_addr(&self) -> Result<EndpointAddr, String> {
        let id = self.endpoint_public_key()?;
        Ok(EndpointAddr::from_parts(
            id,
            self.addrs
                .iter()
                .copied()
                .map(TransportAddr::Ip)
                .collect::<Vec<_>>(),
        ))
    }
}

/// A peer that completed the handshake, as shown in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPeer {
    /// Contact fingerprint the peer proved it belongs to.
    pub fingerprint: String,
    /// Endpoint id it presented.
    pub endpoint_id: String,
    /// Unix-epoch seconds of the most recent frame.
    pub last_seen: u64,
}

/// Something a peer sent us, awaiting the session's next sync.
#[derive(Debug, Clone, PartialEq)]
pub enum PeerInbound {
    /// A signed object (message, post, profile) from a contact.
    Object {
        fingerprint: String,
        endpoint_id: String,
        object: Value,
    },
    /// A change notice from a contact.
    Notice {
        fingerprint: String,
        endpoint_id: String,
        kind: UpdateKind,
    },
}

/// A certificate we accepted, so the session can persist the pin for future sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedCertificate {
    pub fingerprint: String,
    pub endpoint_id: String,
    pub issued_at: u64,
}

/// Snapshot of the peer subsystem, rendered by frontends under the `peers` key.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PeerStatus {
    pub active: bool,
    /// Our device endpoint id (hex). Not the profile key; see ADR 0003.
    pub node_id: Option<String>,
    pub peer_count: usize,
    /// Discovery mode in effect: `dns-pkarr` or `off`.
    pub discovery: String,
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// DHT device descriptors (M6.5 fallback lookup)
// ---------------------------------------------------------------------------

/// DHT namespace under which a profile publishes its current device endpoint.
pub const DEVICE_NAMESPACE: &str = "snartnet/device";

/// Maximum serialized descriptor size. BEP-44 refuses values above 1000 bytes, so the
/// descriptor has to stay well inside that budget; it is public data, not a secret.
pub const MAX_DESCRIPTOR_BYTES: usize = 1000;

/// A pointer from a profile to one device endpoint, published as a signed DHT record.
///
/// The record itself is signed by the profile key (BEP-44 mutable records are keyed by the
/// profile public key), and it embeds a [`DeviceCertificate`], so a contact can discover
/// *which* endpoint to dial without trusting the DHT node that served the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    pub v: u16,
    pub profile: String,
    pub endpoint_id: String,
    pub certificate: DeviceCertificate,
    /// Advertised TCP sync address, so the same record also feeds the torrent path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_addr: Option<String>,
    pub updated_at: u64,
}

impl DeviceDescriptor {
    /// Build a descriptor from an already-issued certificate.
    pub fn new(certificate: DeviceCertificate, tcp_addr: Option<String>, updated_at: u64) -> Self {
        Self {
            v: 1,
            profile: certificate.profile.clone(),
            endpoint_id: certificate.endpoint_id.clone(),
            certificate,
            tcp_addr,
            updated_at,
        }
    }

    /// Serialize the descriptor, refusing anything BEP-44 would reject.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let bytes =
            serde_json::to_vec(self).map_err(|e| format!("device descriptor encoding: {e}"))?;
        if bytes.len() > MAX_DESCRIPTOR_BYTES {
            return Err(format!(
                "device descriptor is {} bytes, over the {MAX_DESCRIPTOR_BYTES} byte DHT limit",
                bytes.len()
            ));
        }
        Ok(bytes)
    }

    /// Parse a descriptor fetched from the DHT.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("device descriptor decoding: {e}"))
    }

    /// Validate the descriptor against the contact we asked about and turn it into a dial target.
    ///
    /// The DHT is untrusted: the record is only usable because the embedded certificate
    /// verifies under the profile key whose fingerprint matches `expected_fingerprint`, and
    /// because the descriptor's own identity fields agree with the certificate.
    pub fn to_target(&self, expected_fingerprint: &str, now: u64) -> Result<PeerTarget, PeerError> {
        if self.v != 1 {
            return Err(PeerError::Handshake(format!(
                "unsupported device descriptor version {}",
                self.v
            )));
        }
        if self.profile != expected_fingerprint {
            return Err(PeerError::Certificate(CertificateError::ProfileMismatch {
                claimed: self.profile.clone(),
                derived: expected_fingerprint.to_string(),
            }));
        }
        let endpoint = self
            .certificate
            .endpoint_public_key()
            .map_err(PeerError::Certificate)?;
        if self.endpoint_id != self.certificate.endpoint_id {
            return Err(PeerError::Handshake(
                "descriptor endpoint does not match its certificate".into(),
            ));
        }
        self.certificate
            .verify_for_profile(expected_fingerprint, &endpoint, now)
            .map_err(PeerError::Certificate)?;
        if !self.certificate.has_capability(CAPABILITY_PEER) {
            return Err(PeerError::Certificate(CertificateError::MissingCapability(
                CAPABILITY_PEER.to_string(),
            )));
        }
        let mut addrs = Vec::new();
        if let Some(addr) = &self.tcp_addr {
            if let Ok(parsed) = addr.parse::<SocketAddr>() {
                addrs.push(parsed);
            }
        }
        Ok(PeerTarget {
            fingerprint: self.profile.clone(),
            endpoint_id: self.endpoint_id.clone(),
            addrs,
        })
    }
}

// ---------------------------------------------------------------------------
// Accept side
// ---------------------------------------------------------------------------

/// Shared state behind both the accept handler and the public handle.
struct PeerInner {
    certificate: DeviceCertificate,
    /// Contact policies by fingerprint. A credential we do not hold here cannot talk to us.
    policy: Mutex<BTreeMap<String, ContactPolicy>>,
    peers: Mutex<Vec<AuthenticatedPeer>>,
    inbox: Mutex<Vec<PeerInbound>>,
    accepted: Mutex<Vec<AcceptedCertificate>>,
    targets: Mutex<Vec<PeerTarget>>,
    active: AtomicBool,
    status: Mutex<PeerStatus>,
}

impl PeerInner {
    fn new(certificate: DeviceCertificate, options: PeerOptions) -> Self {
        Self {
            certificate,
            policy: Mutex::new(BTreeMap::new()),
            peers: Mutex::new(Vec::new()),
            inbox: Mutex::new(Vec::new()),
            accepted: Mutex::new(Vec::new()),
            targets: Mutex::new(Vec::new()),
            active: AtomicBool::new(false),
            status: Mutex::new(PeerStatus {
                active: false,
                node_id: None,
                peer_count: 0,
                discovery: discovery_label(options.discovery).to_string(),
                last_error: None,
            }),
        }
    }

    fn set_error(&self, message: Option<String>) {
        if let Ok(mut status) = self.status.lock() {
            status.last_error = message;
        }
    }

    fn policy_for(&self, fingerprint: &str) -> Option<ContactPolicy> {
        self.policy.lock().ok()?.get(fingerprint).cloned()
    }

    /// Replace the contact policies. Pins learned in this session are preserved.
    fn set_contacts(&self, policies: Vec<ContactPolicy>) {
        let Ok(mut current) = self.policy.lock() else {
            return;
        };
        let mut next: BTreeMap<String, ContactPolicy> = policies
            .into_iter()
            .map(|policy| (policy.fingerprint.clone(), policy))
            .collect();
        for (fingerprint, previous) in current.iter() {
            let Some(policy) = next.get_mut(fingerprint) else {
                continue;
            };
            // Keep a pin that is at least as new as the one the caller supplied, so a
            // concurrent sync cannot roll replay protection backwards.
            let keep = previous.pinned_endpoint.is_some()
                && previous.pinned_issued_at >= policy.pinned_issued_at;
            if policy.pinned_issued_at.is_none() || keep {
                policy.pinned_endpoint = previous.pinned_endpoint.clone();
                policy.pinned_issued_at = previous.pinned_issued_at;
            }
        }
        *current = next;
    }

    /// Validate a peer's hello against our contact policy and the connection identity.
    ///
    /// The `remote` endpoint id comes from the TLS handshake, so it is the one input a peer
    /// cannot choose freely: a certificate for someone else's device fails here even when
    /// its signature is genuine.
    fn validate_hello(
        &self,
        certificate: &DeviceCertificate,
        remote: &EndpointId,
        now: u64,
    ) -> Result<ContactPolicy, PeerError> {
        let policy = self
            .policy_for(&certificate.profile)
            .ok_or_else(|| PeerError::NotAContact(certificate.profile.clone()))?;
        certificate
            .verify_with_capability(&policy.fingerprint, remote, CAPABILITY_PEER, now)
            .map_err(PeerError::Certificate)?;
        if let Some(pin) = policy.pin() {
            pin.check(certificate).map_err(PeerError::Certificate)?;
        }
        Ok(policy)
    }

    /// Remember an accepted certificate in memory, and hand it to the session for persistence.
    fn record_acceptance(&self, certificate: &DeviceCertificate, endpoint: &EndpointId, now: u64) {
        let fingerprint = certificate.profile.clone();
        let endpoint_id = endpoint.to_string();
        if let Ok(mut accepted) = self.accepted.lock() {
            accepted.push(AcceptedCertificate {
                fingerprint: fingerprint.clone(),
                endpoint_id: endpoint_id.clone(),
                issued_at: certificate.issued_at,
            });
        }
        if let Ok(mut policy) = self.policy.lock() {
            if let Some(existing) = policy.get_mut(&fingerprint) {
                if existing.pinned_issued_at.unwrap_or(0) <= certificate.issued_at {
                    existing.pinned_endpoint = Some(endpoint_id.clone());
                    existing.pinned_issued_at = Some(certificate.issued_at);
                }
            }
        }
        if let Ok(mut peers) = self.peers.lock() {
            match peers
                .iter_mut()
                .find(|peer| peer.fingerprint == fingerprint)
            {
                Some(peer) => {
                    peer.endpoint_id = endpoint_id;
                    peer.last_seen = now;
                }
                None => {
                    if peers.len() < MAX_PEERS {
                        peers.push(AuthenticatedPeer {
                            fingerprint,
                            endpoint_id,
                            last_seen: now,
                        });
                    }
                }
            }
        }
        self.refresh_peer_count();
    }

    fn refresh_peer_count(&self) {
        let live = {
            let Ok(mut peers) = self.peers.lock() else {
                return;
            };
            peers.retain(|peer| is_peer_fresh(peer.last_seen));
            peers.len()
        };
        if let Ok(mut status) = self.status.lock() {
            status.peer_count = live;
        }
    }

    fn push_inbound(&self, inbound: PeerInbound) {
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox.push(inbound);
        }
    }

    /// Serve one accepted connection: handshake first, application frames second.
    async fn handle_connection(&self, connection: Connection) -> Result<(), PeerError> {
        let remote = connection.remote_id();
        let result = self.serve(&connection, &remote).await;
        if let Err(error) = &result {
            let message = error.to_string();
            let reason: Vec<u8> = message.as_bytes().iter().take(120).copied().collect();
            connection.close(VarInt::from_u32(close_code(error)), &reason);
            self.set_error(Some(message));
        }
        result
    }

    async fn serve(&self, connection: &Connection, remote: &EndpointId) -> Result<(), PeerError> {
        let (mut send, mut recv) = tokio::time::timeout(HANDSHAKE_TIMEOUT, connection.accept_bi())
            .await
            .map_err(|_| PeerError::Timeout)?
            .map_err(|e| PeerError::Transport(e.to_string()))?;

        let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(&mut recv, MAX_FRAME_BYTES))
            .await
            .map_err(|_| PeerError::Timeout)??
            .ok_or(PeerError::StreamClosed)?;
        let (certificate, nonce) = match first {
            Frame::Hello { certificate, nonce } => (certificate, nonce),
            _ => return Err(PeerError::ExpectedHello),
        };
        let now = lan_unix_secs();
        self.validate_hello(&certificate, remote, now)?;
        write_frame(
            &mut send,
            &Frame::HelloAck {
                certificate: self.certificate.clone(),
                echo: nonce,
            },
        )
        .await?;
        self.record_acceptance(&certificate, remote, now);
        self.set_error(None);

        let peer_fingerprint = certificate.profile;
        let endpoint_id = remote.to_string();
        let mut received: u32 = 0;
        loop {
            let next =
                tokio::time::timeout(DELIVERY_TIMEOUT, read_frame(&mut recv, MAX_FRAME_BYTES))
                    .await
                    .map_err(|_| PeerError::Timeout)??;
            match next {
                None => break,
                Some(Frame::Object { object }) => {
                    self.push_inbound(PeerInbound::Object {
                        fingerprint: peer_fingerprint.clone(),
                        endpoint_id: endpoint_id.clone(),
                        object,
                    });
                    received += 1;
                }
                Some(Frame::Notice { kind, fingerprint }) => {
                    // A notice can only speak for the profile the certificate proved.
                    if fingerprint != peer_fingerprint {
                        return Err(PeerError::Handshake(
                            "notice names a different profile than the certificate".into(),
                        ));
                    }
                    self.push_inbound(PeerInbound::Notice {
                        fingerprint,
                        endpoint_id: endpoint_id.clone(),
                        kind,
                    });
                    received += 1;
                }
                Some(Frame::Goodbye) => break,
                Some(other) => {
                    return Err(PeerError::Malformed(format!(
                        "unexpected {other:?} during an established conversation"
                    )))
                }
            }
        }
        // The sender waits for this before treating an object as delivered, so it is written
        // only after every frame is queued in the inbox.
        write_frame(&mut send, &Frame::Ack { frames: received }).await?;
        let _ = send.finish();
        // `send.finish` closes our half of the stream only: the acknowledgement is still in
        // flight, and returning here would drop the connection and the ack with it. Waiting
        // for the sender to hang up is what makes its read of the ack reliable.
        let _ = tokio::time::timeout(ACK_LINGER_TIMEOUT, connection.closed()).await;
        self.refresh_peer_count();
        Ok(())
    }
}

/// Accept-side handler for [`PEER_ALPN`].
///
/// Iroh requires handlers to be `Debug`, so the debug output stays limited to the device
/// endpoint id: the certificate and the peer bookkeeping are never formatted.
#[derive(Clone)]
struct PeerProtocol {
    inner: Arc<PeerInner>,
}

impl fmt::Debug for PeerProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerProtocol")
            .field("endpoint_id", &self.inner.certificate.endpoint_id)
            .finish_non_exhaustive()
    }
}

impl ProtocolHandler for PeerProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        match self.inner.handle_connection(connection).await {
            Ok(()) => Ok(()),
            // The refusal already closed the connection with a specific code and recorded
            // the reason in the peer status, so the router does not need to log it again.
            Err(_) => Ok(()),
        }
    }
}

/// QUIC application close code for a refusal reason.
fn close_code(error: &PeerError) -> u32 {
    match error {
        PeerError::NotAContact(_) => CLOSE_UNKNOWN_CONTACT,
        PeerError::Certificate(CertificateError::Superseded { .. }) => CLOSE_STALE_CERTIFICATE,
        PeerError::Certificate(_) => CLOSE_BAD_CERTIFICATE,
        _ => CLOSE_PROTOCOL_ERROR,
    }
}

/// Label for the discovery strategy, shown in the status snapshot.
pub fn discovery_label(mode: DiscoveryMode) -> &'static str {
    match mode {
        DiscoveryMode::DnsPkarr => "dns-pkarr",
        DiscoveryMode::Off => "off",
    }
}

#[inline]
fn is_peer_fresh(last_seen: u64) -> bool {
    lan_unix_secs().saturating_sub(last_seen) < PEER_EXPIRY_SECS
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

/// An iroh endpoint that speaks [`PEER_ALPN`] with contacts only.
pub struct PeerNode {
    runtime: Arc<Runtime>,
    endpoint: Endpoint,
    inner: Arc<PeerInner>,
    _router: Router,
}

impl PeerNode {
    /// Open the endpoint using the environment's discovery and relay settings.
    pub fn open(device: &DeviceKey, certificate: DeviceCertificate) -> Result<Self, String> {
        Self::open_with(device, certificate, PeerOptions::from_env())
    }

    /// Open the endpoint with explicit options.
    ///
    /// The endpoint key is `device`, never the profile signing key (ADR 0003); the
    /// certificate is what tells contacts that this endpoint belongs to the profile.
    pub fn open_with(
        device: &DeviceKey,
        certificate: DeviceCertificate,
        options: PeerOptions,
    ) -> Result<Self, String> {
        if certificate.endpoint_id != device.endpoint_id_string() {
            return Err("the device certificate names a different endpoint".into());
        }
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .map_err(|e| format!("peer runtime failed: {e}"))?,
        );
        let discovery = options.discovery;
        let mut builder = Endpoint::builder(presets::Minimal)
            .secret_key(device.secret_key().clone())
            .relay_mode(options.relay.clone())
            .alpns(vec![PEER_ALPN.to_vec()]);
        if discovery == DiscoveryMode::DnsPkarr {
            // M6.5: publish our endpoint to iroh's n0 DNS/Pkarr service and resolve contacts
            // through it, over both Pkarr/HTTPS and plain DNS.
            builder = builder
                .address_lookup(PkarrPublisher::n0_dns())
                .address_lookup(PkarrResolver::n0_dns())
                .address_lookup(DnsAddressLookup::n0_dns());
        }
        let inner = Arc::new(PeerInner::new(certificate, options));
        let (endpoint, router) = runtime.block_on(async {
            let endpoint = builder
                .bind()
                .await
                .map_err(|e| format!("iroh endpoint bind failed: {e}"))?;
            let router = Router::builder(endpoint.clone())
                .accept(
                    PEER_ALPN,
                    PeerProtocol {
                        inner: inner.clone(),
                    },
                )
                .spawn();
            Ok::<_, String>((endpoint, router))
        })?;
        if let Ok(mut status) = inner.status.lock() {
            status.node_id = Some(endpoint.id().to_string());
        }
        Ok(Self {
            runtime,
            endpoint,
            inner,
            _router: router,
        })
    }

    /// Our device endpoint id (hex). Not the profile key.
    pub fn node_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// Addresses this endpoint is bound to, for tests and diagnostics.
    pub fn bound_sockets(&self) -> Vec<SocketAddr> {
        self.endpoint.bound_sockets()
    }

    /// Replace the set of contacts allowed to talk to us.
    pub fn set_contacts(&self, policies: Vec<ContactPolicy>) {
        self.inner.set_contacts(policies);
    }

    /// Start the contact-scoped presence loop over `targets`.
    ///
    /// There is no topic to join: presence only ever reaches devices we already know, and
    /// every dial completes a certificate handshake before a notice is accepted.
    pub fn start(&self, targets: Vec<PeerTarget>) {
        self.stop();
        self.inner.active.store(true, Ordering::Relaxed);
        if let Ok(mut stored) = self.inner.targets.lock() {
            *stored = targets;
        }
        if let Ok(mut status) = self.inner.status.lock() {
            status.active = true;
            status.last_error = None;
        }
        let handle = self.handle();
        self.runtime.spawn(async move {
            while handle.inner.active.load(Ordering::Relaxed) {
                let targets = handle
                    .inner
                    .targets
                    .lock()
                    .map(|targets| targets.clone())
                    .unwrap_or_default();
                for target in targets {
                    let notice = vec![Frame::Notice {
                        kind: UpdateKind::Presence,
                        fingerprint: target.fingerprint.clone(),
                    }];
                    let _ = handle.deliver(&target, notice).await;
                }
                let checks = PRESENCE_INTERVAL_SECS * 1000 / SHUTDOWN_CHECK_INTERVAL_MS;
                for _ in 0..checks {
                    if !handle.inner.active.load(Ordering::Relaxed) {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(SHUTDOWN_CHECK_INTERVAL_MS)).await;
                }
            }
        });
    }

    /// Stop the presence loop and forget the peers seen so far.
    pub fn stop(&self) {
        self.inner.active.store(false, Ordering::Relaxed);
        if let Ok(mut peers) = self.inner.peers.lock() {
            peers.clear();
        }
        if let Ok(mut status) = self.inner.status.lock() {
            status.active = false;
            status.peer_count = 0;
        }
    }

    /// Whether the presence loop is running.
    pub fn is_active(&self) -> bool {
        self.inner.active.load(Ordering::Relaxed)
    }

    /// Snapshot for the daemon's `peers` state key.
    pub fn status(&self) -> PeerStatus {
        self.inner
            .status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    /// Contacts that completed a handshake recently.
    pub fn peers(&self) -> Vec<AuthenticatedPeer> {
        self.inner.refresh_peer_count();
        self.inner
            .peers
            .lock()
            .map(|peers| peers.clone())
            .unwrap_or_default()
    }

    /// Take every frame received since the last call.
    pub fn drain_inbox(&self) -> Vec<PeerInbound> {
        self.inner
            .inbox
            .lock()
            .map(|mut inbox| std::mem::take(&mut *inbox))
            .unwrap_or_default()
    }

    /// Take the certificates accepted since the last call, for the session to persist.
    pub fn take_accepted(&self) -> Vec<AcceptedCertificate> {
        self.inner
            .accepted
            .lock()
            .map(|mut accepted| std::mem::take(&mut *accepted))
            .unwrap_or_default()
    }

    /// A handle for background tasks and one-shot deliveries.
    fn handle(&self) -> Arc<PeerHandle> {
        Arc::new(PeerHandle {
            endpoint: self.endpoint.clone(),
            inner: self.inner.clone(),
        })
    }

    /// Deliver an object to one contact device, blocking until it is acknowledged.
    pub fn send_object(&self, target: &PeerTarget, object: &Value) -> bool {
        self.send_frames(
            target,
            vec![Frame::Object {
                object: object.clone(),
            }],
        )
        .is_some()
    }

    /// Deliver frames to one contact device. `Some(count)` is the peer's acknowledgement.
    pub fn send_frames(&self, target: &PeerTarget, frames: Vec<Frame>) -> Option<u32> {
        let handle = self.handle();
        let target = target.clone();
        self.runtime
            .block_on(async move { handle.deliver(&target, frames).await.ok() })
    }

    /// Deliver a change notice to every target that accepts it, returning how many did.
    pub fn announce(&self, kind: UpdateKind, targets: Vec<PeerTarget>) -> usize {
        targets
            .into_iter()
            .filter(|target| {
                self.send_frames(
                    target,
                    vec![Frame::Notice {
                        kind,
                        fingerprint: target.fingerprint.clone(),
                    }],
                )
                .is_some()
            })
            .count()
    }
}

/// The subset of a [`PeerNode`] needed by background tasks and one-shot deliveries.
struct PeerHandle {
    endpoint: Endpoint,
    inner: Arc<PeerInner>,
}

impl PeerHandle {
    /// Connect, authenticate both sides, deliver `frames`, and wait for the peer's ack.
    async fn deliver(&self, target: &PeerTarget, frames: Vec<Frame>) -> Result<u32, PeerError> {
        let addr = target.endpoint_addr().map_err(PeerError::Transport)?;
        let connection =
            tokio::time::timeout(HANDSHAKE_TIMEOUT, self.endpoint.connect(addr, PEER_ALPN))
                .await
                .map_err(|_| PeerError::Timeout)?
                .map_err(|e| PeerError::Transport(e.to_string()))?;
        let remote = connection.remote_id();
        let result = self
            .serve_outbound(&connection, &remote, target, frames)
            .await;
        connection.close(0u32.into(), b"done");
        result
    }

    async fn serve_outbound(
        &self,
        connection: &Connection,
        remote: &EndpointId,
        target: &PeerTarget,
        frames: Vec<Frame>,
    ) -> Result<u32, PeerError> {
        if remote.to_string() != target.endpoint_id {
            // iroh's TLS proof says we reached a different endpoint than we asked for, so
            // the certificate check below would compare against the wrong identity.
            return Err(PeerError::Handshake(
                "connected to a different endpoint than the target".into(),
            ));
        }
        let (mut send, mut recv) = tokio::time::timeout(HANDSHAKE_TIMEOUT, connection.open_bi())
            .await
            .map_err(|_| PeerError::Timeout)?
            .map_err(|e| PeerError::Transport(e.to_string()))?;
        let nonce = connection_nonce();
        write_frame(
            &mut send,
            &Frame::Hello {
                certificate: self.inner.certificate.clone(),
                nonce: nonce.clone(),
            },
        )
        .await?;
        let answer =
            tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(&mut recv, MAX_FRAME_BYTES))
                .await
                .map_err(|_| PeerError::Timeout)??
                .ok_or(PeerError::StreamClosed)?;
        let (certificate, echo) = match answer {
            Frame::HelloAck { certificate, echo } => (certificate, echo),
            _ => return Err(PeerError::ExpectedHello),
        };
        // The echo proves this ack belongs to this connection, so a recorded handshake
        // cannot be replayed into a different one.
        if echo != nonce {
            return Err(PeerError::NonceMismatch);
        }
        let now = lan_unix_secs();
        certificate
            .verify_with_capability(&target.fingerprint, remote, CAPABILITY_PEER, now)
            .map_err(PeerError::Certificate)?;
        self.inner.record_acceptance(&certificate, remote, now);
        self.inner.set_error(None);

        for frame in frames {
            write_frame(&mut send, &frame).await?;
        }
        let _ = send.finish();
        let ack = tokio::time::timeout(DELIVERY_TIMEOUT, read_frame(&mut recv, MAX_FRAME_BYTES))
            .await
            .map_err(|_| PeerError::Timeout)??;
        match ack {
            Some(Frame::Ack { frames }) => Ok(frames),
            Some(_) => Err(PeerError::Handshake(
                "the peer acknowledged with an unexpected frame".into(),
            )),
            None => Err(PeerError::StreamClosed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{CAPABILITY_CHAT, DEFAULT_CAPABILITIES, DEVICE_CERT_TTL_SECS};
    use serde_json::json;
    use snartnet_core::{KeyInfo, KeyPair, Profile, SignedProfile};
    use std::net::{IpAddr, Ipv4Addr};

    /// A fixed clock for the pure certificate cases. Live endpoints are validated against the
    /// real clock, so those tests issue certificates at [`lan_unix_secs`] instead.
    const NOW: u64 = 1_000;

    /// A profile with one certified device: everything a contact needs to dial and be dialed.
    struct Identity {
        profile: SignedProfile,
        keypair: KeyPair,
        key: DeviceKey,
        certificate: DeviceCertificate,
    }

    impl Identity {
        fn new(name: &str, issued_at: u64) -> Self {
            let keypair = KeyPair::generate().unwrap();
            let info: KeyInfo = keypair.get_public_info();
            let profile = SignedProfile::create(Profile::new(name.into(), info), &keypair).unwrap();
            let key = DeviceKey::generate();
            let certificate = Self::certify(&profile, &keypair, &key, issued_at);
            Self {
                profile,
                keypair,
                key,
                certificate,
            }
        }

        /// Re-issue this profile's certificate for its existing device, as a renewal would.
        fn renewal(&self, issued_at: u64) -> DeviceCertificate {
            Self::certify(&self.profile, &self.keypair, &self.key, issued_at)
        }

        fn certify(
            profile: &SignedProfile,
            keypair: &KeyPair,
            key: &DeviceKey,
            issued_at: u64,
        ) -> DeviceCertificate {
            DeviceCertificate::issue(
                profile,
                keypair,
                key,
                &DEFAULT_CAPABILITIES,
                issued_at,
                DEVICE_CERT_TTL_SECS,
            )
            .unwrap()
        }

        /// An identity whose certificate is valid against the real clock, for live handshakes.
        fn live(name: &str) -> Self {
            Self::new(name, lan_unix_secs())
        }

        fn fingerprint(&self) -> String {
            self.profile.profile.fingerprint.clone()
        }

        fn policy(&self) -> ContactPolicy {
            ContactPolicy::new(&self.fingerprint(), &self.profile.profile.public_key)
        }

        fn node(&self) -> PeerNode {
            PeerNode::open_with(&self.key, self.certificate.clone(), PeerOptions::local())
                .expect("a local endpoint binds")
        }
    }

    /// Socket addresses a peer on this machine can actually dial.
    ///
    /// A wildcard bind (`0.0.0.0`) is reachable through loopback on the same port, which is
    /// how two endpoints in one test process find each other without a network.
    fn usable_addrs(sockets: Vec<SocketAddr>) -> Vec<SocketAddr> {
        let usable: Vec<SocketAddr> = sockets
            .iter()
            .copied()
            .filter(|addr| !addr.ip().is_unspecified())
            .collect();
        if usable.is_empty() {
            sockets
                .iter()
                .map(|addr| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port()))
                .collect()
        } else {
            usable
        }
    }

    fn dialable(node: &PeerNode) -> Vec<SocketAddr> {
        usable_addrs(node.bound_sockets())
    }

    /// The address another local endpoint dials: the endpoint id plus its bound sockets.
    fn endpoint_addr(endpoint: &Endpoint, sockets: Vec<SocketAddr>) -> EndpointAddr {
        EndpointAddr::from_parts(
            endpoint.id(),
            sockets
                .into_iter()
                .map(TransportAddr::Ip)
                .collect::<Vec<_>>(),
        )
    }

    /// The target that lets `dialer` reach `identity`'s already-open endpoint.
    fn target_for(identity: &Identity, node: &PeerNode) -> PeerTarget {
        PeerTarget {
            fingerprint: identity.fingerprint(),
            endpoint_id: node.node_id(),
            addrs: dialable(node),
        }
    }

    /// A handle onto a node's internals, so a test can read the exact handshake error
    /// instead of the `bool` the public delivery API reduces it to.
    fn handle(node: &PeerNode) -> PeerHandle {
        PeerHandle {
            endpoint: node.endpoint.clone(),
            inner: node.inner.clone(),
        }
    }

    /// Wait for a node to record a refusal, then return it. The refusal is written after the
    /// connection is closed, so observing the close from the other side does not imply it exists yet.
    fn wait_for_error(node: &PeerNode, needle: &str) -> String {
        for _ in 0..200 {
            if let Some(error) = node.status().last_error {
                assert!(
                    error.contains(needle),
                    "expected an error mentioning {needle:?}, got {error:?}"
                );
                return error;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("the peer never reported an error mentioning {needle:?}");
    }

    // -----------------------------------------------------------------------
    // Framing
    // -----------------------------------------------------------------------

    /// Every frame kind a conversation can carry, so the round-trip test covers the wire format.
    fn sample_frames(certificate: DeviceCertificate) -> Vec<Frame> {
        vec![
            Frame::Hello {
                certificate: certificate.clone(),
                nonce: "nonce-value".into(),
            },
            Frame::HelloAck {
                certificate,
                echo: "nonce-value".into(),
            },
            Frame::Object {
                object: json!({"kind": "post", "content": "hello bob"}),
            },
            Frame::Notice {
                kind: UpdateKind::Presence,
                fingerprint: "fingerprint".into(),
            },
            Frame::Ack { frames: 3 },
            Frame::Goodbye,
        ]
    }

    #[test]
    fn every_frame_kind_round_trips_through_its_length_prefix() {
        let identity = Identity::new("alice", NOW);
        for frame in sample_frames(identity.certificate.clone()) {
            let encoded = encode_frame(&frame).expect("a small frame encodes");
            assert!(encoded.len() > FRAME_HEADER_BYTES);
            let header: [u8; FRAME_HEADER_BYTES] =
                encoded[..FRAME_HEADER_BYTES].try_into().unwrap();
            let payload = &encoded[FRAME_HEADER_BYTES..];
            assert_eq!(
                validated_payload_len(header, MAX_FRAME_BYTES).unwrap(),
                payload.len(),
                "the prefix must state the payload length for {frame:?}"
            );
            assert_eq!(decode_frame(payload).unwrap(), frame);
        }
    }

    #[test]
    fn an_empty_frame_is_refused_before_anything_is_allocated() {
        assert_eq!(
            validated_payload_len([0, 0, 0, 0], MAX_FRAME_BYTES),
            Err(PeerError::EmptyFrame)
        );
        assert_eq!(decode_frame(&[]), Err(PeerError::EmptyFrame));
    }

    #[test]
    fn an_oversize_frame_is_refused_with_the_size_it_promised() {
        let huge = (MAX_FRAME_BYTES + 1) as u32;
        assert_eq!(
            validated_payload_len(huge.to_be_bytes(), MAX_FRAME_BYTES),
            Err(PeerError::FrameTooLarge {
                size: MAX_FRAME_BYTES + 1,
                max: MAX_FRAME_BYTES,
            })
        );
        // The writer refuses to build the frame at all, so the limit cannot be bypassed by
        // handing `write_frame` a huge value.
        let oversized = Frame::Object {
            object: json!({ "content": "x".repeat(MAX_FRAME_BYTES) }),
        };
        assert!(matches!(
            encode_frame(&oversized),
            Err(PeerError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn a_malformed_payload_is_reported_as_malformed() {
        assert!(matches!(
            decode_frame(b"not json"),
            Err(PeerError::Malformed(_))
        ));
        assert!(matches!(
            decode_frame(br#"{"type":"teleport"}"#),
            Err(PeerError::Malformed(_))
        ));
        // A known tag without its payload is not a frame either.
        assert!(matches!(
            decode_frame(br#"{"type":"notice"}"#),
            Err(PeerError::Malformed(_))
        ));
    }

    #[test]
    fn a_nonce_is_sixteen_random_bytes_per_connection() {
        let first = connection_nonce();
        let second = connection_nonce();
        assert_ne!(
            first, second,
            "a reused nonce would make an echo replayable"
        );
        assert_eq!(STANDARD.decode(first).unwrap().len(), 16);
        assert_eq!(STANDARD.decode(second).unwrap().len(), 16);
    }

    // -----------------------------------------------------------------------
    // Refusal rules
    // -----------------------------------------------------------------------

    #[test]
    fn each_refusal_rule_has_its_own_close_code() {
        assert_eq!(
            close_code(&PeerError::NotAContact("someone".into())),
            CLOSE_UNKNOWN_CONTACT
        );
        assert_eq!(
            close_code(&PeerError::Certificate(CertificateError::Superseded {
                presented: 1,
                accepted: 2,
                endpoint_id: "endpoint".into(),
            })),
            CLOSE_STALE_CERTIFICATE
        );
        assert_eq!(
            close_code(&PeerError::Certificate(CertificateError::SignatureInvalid)),
            CLOSE_BAD_CERTIFICATE
        );
        assert_eq!(close_code(&PeerError::ExpectedHello), CLOSE_PROTOCOL_ERROR);
        assert_eq!(close_code(&PeerError::NonceMismatch), CLOSE_PROTOCOL_ERROR);
    }

    #[test]
    fn a_fresh_endpoint_reports_its_discovery_mode_and_no_peers() {
        let identity = Identity::new("alice", NOW);
        let inner = PeerInner::new(identity.certificate.clone(), PeerOptions::local());
        let status = inner.status.lock().unwrap().clone();
        assert!(!status.active);
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.discovery, discovery_label(DiscoveryMode::Off));
        assert_eq!(discovery_label(DiscoveryMode::DnsPkarr), "dns-pkarr");
        assert!(status.node_id.is_none());
        assert!(status.last_error.is_none());
    }

    #[test]
    fn an_endpoint_refuses_a_certificate_for_a_different_device() {
        let alice = Identity::new("alice", NOW);
        let stranger = DeviceKey::generate();
        // The certificate names Alice's device, so binding the endpoint with another key
        // would let it present authority it does not hold.
        match PeerNode::open_with(&stranger, alice.certificate.clone(), PeerOptions::local()) {
            Ok(_) => panic!("a certificate for another device must not bind"),
            Err(error) => assert!(error.contains("different endpoint"), "{error}"),
        }
    }

    // -----------------------------------------------------------------------
    // Contact policy and replay protection (M6.4)
    // -----------------------------------------------------------------------

    #[test]
    fn a_hello_from_a_stranger_is_refused_as_an_unknown_contact() {
        let alice = Identity::new("alice", NOW);
        let stranger = Identity::new("stranger", NOW);
        let inner = PeerInner::new(alice.certificate.clone(), PeerOptions::local());
        let error = inner
            .validate_hello(&stranger.certificate, &stranger.key.endpoint_id(), NOW)
            .unwrap_err();
        assert_eq!(error, PeerError::NotAContact(stranger.fingerprint()));
        assert_eq!(close_code(&error), CLOSE_UNKNOWN_CONTACT);
        assert!(inner.peers.lock().unwrap().is_empty());
    }

    #[test]
    fn a_certificate_for_another_endpoint_is_refused_even_when_signed() {
        let alice = Identity::new("alice", NOW);
        let bob = Identity::new("bob", NOW);
        let inner = PeerInner::new(alice.certificate.clone(), PeerOptions::local());
        inner.set_contacts(vec![bob.policy()]);
        // Bob is a contact and his certificate is genuine, but the TLS identity that
        // presented it is somebody else's device.
        let other_device = DeviceKey::generate();
        let error = inner
            .validate_hello(&bob.certificate, &other_device.endpoint_id(), NOW)
            .unwrap_err();
        assert!(matches!(
            error,
            PeerError::Certificate(CertificateError::EndpointMismatch { .. })
        ));
        assert_eq!(close_code(&error), CLOSE_BAD_CERTIFICATE);
    }

    #[test]
    fn a_replayed_certificate_is_refused_once_a_newer_one_was_accepted() {
        let alice = Identity::new("alice", NOW);
        let bob = Identity::new("bob", NOW);
        let renewed = bob.renewal(NOW + 3_600);
        let mut policy = bob.policy();
        policy.pinned_endpoint = Some(bob.key.endpoint_id_string());
        policy.pinned_issued_at = Some(renewed.issued_at);
        let inner = PeerInner::new(alice.certificate.clone(), PeerOptions::local());
        inner.set_contacts(vec![policy]);
        let error = inner
            .validate_hello(&bob.certificate, &bob.key.endpoint_id(), NOW)
            .unwrap_err();
        assert_eq!(
            error,
            PeerError::Certificate(CertificateError::Superseded {
                presented: bob.certificate.issued_at,
                accepted: renewed.issued_at,
                endpoint_id: bob.key.endpoint_id_string(),
            })
        );
        assert_eq!(close_code(&error), CLOSE_STALE_CERTIFICATE);
        // The renewal itself is still welcome, so pinning does not lock a contact out.
        assert!(inner
            .validate_hello(&renewed, &bob.key.endpoint_id(), renewed.issued_at)
            .is_ok());
    }

    #[test]
    fn an_accepted_certificate_becomes_a_pin_and_survives_a_policy_refresh() {
        let alice = Identity::new("alice", NOW);
        let bob = Identity::new("bob", NOW);
        let inner = PeerInner::new(alice.certificate.clone(), PeerOptions::local());
        inner.set_contacts(vec![bob.policy()]);
        // The acceptance clock is the real one: peers older than `PEER_EXPIRY_SECS` are pruned.
        inner.record_acceptance(&bob.certificate, &bob.key.endpoint_id(), lan_unix_secs());
        assert_eq!(
            inner.accepted.lock().unwrap().clone(),
            vec![AcceptedCertificate {
                fingerprint: bob.fingerprint(),
                endpoint_id: bob.key.endpoint_id_string(),
                issued_at: bob.certificate.issued_at,
            }]
        );
        let authenticated = inner.peers.lock().unwrap().clone();
        assert_eq!(authenticated.len(), 1);
        assert_eq!(authenticated[0].endpoint_id, bob.key.endpoint_id_string());
        // A later sync re-sends policies built from persisted contacts, which carry no pin
        // of their own: the pin learned here must not be rolled back.
        inner.set_contacts(vec![bob.policy()]);
        let policy = inner
            .policy_for(&bob.fingerprint())
            .expect("still a contact");
        assert_eq!(policy.pinned_endpoint, Some(bob.key.endpoint_id_string()));
        assert_eq!(policy.pinned_issued_at, Some(bob.certificate.issued_at));
    }

    #[test]
    fn forgetting_contacts_forgets_their_authority() {
        let alice = Identity::new("alice", NOW);
        let bob = Identity::new("bob", NOW);
        let inner = PeerInner::new(alice.certificate.clone(), PeerOptions::local());
        inner.set_contacts(vec![bob.policy()]);
        assert!(inner
            .validate_hello(&bob.certificate, &bob.key.endpoint_id(), NOW)
            .is_ok());
        inner.set_contacts(Vec::new());
        assert_eq!(
            inner
                .validate_hello(&bob.certificate, &bob.key.endpoint_id(), NOW)
                .unwrap_err(),
            PeerError::NotAContact(bob.fingerprint())
        );
    }

    // -----------------------------------------------------------------------
    // DHT device descriptors (M6.5)
    // -----------------------------------------------------------------------

    #[test]
    fn a_device_descriptor_only_yields_a_target_for_the_profile_that_signed_it() {
        let alice = Identity::new("alice", NOW);
        let bob = Identity::new("bob", NOW);
        let descriptor = DeviceDescriptor::new(
            alice.certificate.clone(),
            Some("127.0.0.1:47474".into()),
            NOW,
        );
        let bytes = descriptor
            .to_bytes()
            .expect("a descriptor fits the BEP-44 value budget");
        assert!(bytes.len() <= MAX_DESCRIPTOR_BYTES);
        assert_eq!(DeviceDescriptor::from_bytes(&bytes).unwrap(), descriptor);

        let target = descriptor.to_target(&alice.fingerprint(), NOW).unwrap();
        assert_eq!(target.fingerprint, alice.fingerprint());
        assert_eq!(target.endpoint_id, alice.key.endpoint_id_string());
        assert_eq!(target.addrs, vec!["127.0.0.1:47474".parse().unwrap()]);
        assert_eq!(
            target.endpoint_public_key().unwrap(),
            alice.key.endpoint_id()
        );

        // The DHT node that served the record is untrusted, so another profile cannot adopt it.
        assert_eq!(
            descriptor.to_target(&bob.fingerprint(), NOW).unwrap_err(),
            PeerError::Certificate(CertificateError::ProfileMismatch {
                claimed: alice.fingerprint(),
                derived: bob.fingerprint(),
            })
        );
        // Nor can a record with an expired certificate produce a device to dial.
        assert!(descriptor
            .to_target(&alice.fingerprint(), NOW + DEVICE_CERT_TTL_SECS)
            .is_err());
    }

    #[test]
    fn a_tampered_device_descriptor_is_refused() {
        let alice = Identity::new("alice", NOW);
        let bob = Identity::new("bob", NOW);
        let descriptor = DeviceDescriptor::new(alice.certificate.clone(), None, NOW);

        let mut future = descriptor.clone();
        future.v = 2;
        assert!(matches!(
            future.to_target(&alice.fingerprint(), NOW),
            Err(PeerError::Handshake(reason)) if reason.contains("version")
        ));

        let mut relabelled = descriptor.clone();
        relabelled.endpoint_id = bob.key.endpoint_id_string();
        assert!(matches!(
            relabelled.to_target(&alice.fingerprint(), NOW),
            Err(PeerError::Handshake(reason)) if reason.contains("certificate")
        ));

        let mut extended = descriptor.clone();
        extended.certificate.expires_at += 86_400;
        assert_eq!(
            extended.to_target(&alice.fingerprint(), NOW).unwrap_err(),
            PeerError::Certificate(CertificateError::SignatureInvalid)
        );

        // A device certified for chat only may not be handed to the peer protocol.
        let chat_only = DeviceCertificate::issue(
            &alice.profile,
            &alice.keypair,
            &alice.key,
            &[CAPABILITY_CHAT],
            NOW,
            DEVICE_CERT_TTL_SECS,
        )
        .unwrap();
        let restricted = DeviceDescriptor::new(chat_only, None, NOW);
        assert_eq!(
            restricted.to_target(&alice.fingerprint(), NOW).unwrap_err(),
            PeerError::Certificate(CertificateError::MissingCapability(
                CAPABILITY_PEER.to_string()
            ))
        );
    }

    // -----------------------------------------------------------------------
    // Live handshakes between local endpoints
    // -----------------------------------------------------------------------

    #[test]
    fn two_contacts_authenticate_and_exchange_an_object() {
        let alice = Identity::live("alice");
        let bob = Identity::live("bob");
        let alice_node = alice.node();
        let bob_node = bob.node();
        alice_node.set_contacts(vec![bob.policy()]);
        bob_node.set_contacts(vec![alice.policy()]);

        let object = json!({"kind": "post", "content": "hello bob"});
        let target = target_for(&bob, &bob_node);
        let delivery = alice_node.runtime.block_on(handle(&alice_node).deliver(
            &target,
            vec![Frame::Object {
                object: object.clone(),
            }],
        ));
        if let Err(error) = &delivery {
            panic!(
                "delivery failed: {error} (alice: {:?}, bob: {:?})",
                alice_node.status(),
                bob_node.status()
            );
        }
        // The acknowledgement counts the frames the peer actually queued.
        assert_eq!(delivery.unwrap(), 1);

        assert_eq!(
            bob_node.drain_inbox(),
            vec![PeerInbound::Object {
                fingerprint: alice.fingerprint(),
                endpoint_id: alice_node.node_id(),
                object: object.clone(),
            }]
        );
        // Draining takes the frames, so a second sync cannot ingest the same object twice.
        assert!(bob_node.drain_inbox().is_empty());

        // Each side learned the other's device endpoint, which is what makes the direct
        // path reusable and what gets persisted as a pin.
        assert_eq!(
            alice_node.take_accepted(),
            vec![AcceptedCertificate {
                fingerprint: bob.fingerprint(),
                endpoint_id: bob_node.node_id(),
                issued_at: bob.certificate.issued_at,
            }]
        );
        assert_eq!(
            bob_node.take_accepted(),
            vec![AcceptedCertificate {
                fingerprint: alice.fingerprint(),
                endpoint_id: alice_node.node_id(),
                issued_at: alice.certificate.issued_at,
            }]
        );
        assert_eq!(bob_node.peers().len(), 1);
        assert_eq!(bob_node.peers()[0].endpoint_id, alice_node.node_id());
        assert_eq!(bob_node.status().peer_count, 1);
        assert!(bob_node.status().last_error.is_none());
        assert!(alice_node.status().last_error.is_none());
    }

    #[test]
    fn a_stopped_endpoint_forgets_the_peers_it_authenticated() {
        let alice = Identity::live("alice");
        let node = alice.node();
        node.inner.record_acceptance(
            &alice.certificate.clone(),
            &alice.key.endpoint_id(),
            lan_unix_secs(),
        );
        assert_eq!(node.peers().len(), 1);
        assert_eq!(node.status().peer_count, 1);
        node.stop();
        assert!(!node.is_active());
        assert!(node.peers().is_empty());
        assert_eq!(node.status().peer_count, 0);
    }

    #[test]
    fn a_stranger_is_closed_as_an_unknown_contact() {
        let alice = Identity::live("alice");
        let bob = Identity::live("bob");
        let alice_node = alice.node();
        let bob_node = bob.node();
        // Alice added nobody, so Bob's genuine certificate has no policy to match.
        let refused = bob_node.runtime.block_on(
            handle(&bob_node).deliver(&target_for(&alice, &alice_node), vec![Frame::Goodbye]),
        );
        assert!(
            refused.is_err(),
            "a refused handshake must not look like a delivery"
        );
        let recorded = wait_for_error(&alice_node, "not a known contact");
        assert!(recorded.contains(&bob.fingerprint()));
        assert!(alice_node.peers().is_empty());
        assert!(alice_node.take_accepted().is_empty());
        assert!(alice_node.drain_inbox().is_empty());
    }

    /// Accept-side handler that answers the handshake with a nonce that never arrived.
    #[derive(Clone, Debug)]
    struct WrongEcho {
        certificate: DeviceCertificate,
    }

    impl ProtocolHandler for WrongEcho {
        async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
            let Ok((mut send, mut recv)) = connection.accept_bi().await else {
                return Ok(());
            };
            let Ok(Some(Frame::Hello { .. })) = read_frame(&mut recv, MAX_FRAME_BYTES).await else {
                return Ok(());
            };
            let _ = write_frame(
                &mut send,
                &Frame::HelloAck {
                    certificate: self.certificate.clone(),
                    echo: connection_nonce(),
                },
            )
            .await;
            let _ = send.finish();
            // Returning here would close the connection before the dialer reads the ack, so
            // keep reading until the dialer gives up and closes its side.
            while let Ok(Some(_)) = read_frame(&mut recv, MAX_FRAME_BYTES).await {}
            Ok(())
        }
    }

    /// A bare local endpoint for `identity`: it answers every handshake with a wrong echo, and
    /// can also be used to dial a real node.
    struct FakePeer {
        runtime: Runtime,
        endpoint: Endpoint,
        _router: Router,
    }

    impl FakePeer {
        fn open(identity: &Identity) -> Self {
            let runtime = Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("a test runtime");
            let handler = WrongEcho {
                certificate: identity.certificate.clone(),
            };
            let (endpoint, router) = runtime.block_on(async {
                let endpoint = Endpoint::builder(presets::Minimal)
                    .secret_key(identity.key.secret_key().clone())
                    .relay_mode(RelayMode::Disabled)
                    .alpns(vec![PEER_ALPN.to_vec()])
                    .bind()
                    .await
                    .expect("a local endpoint binds");
                let router = Router::builder(endpoint.clone())
                    .accept(PEER_ALPN, handler)
                    .spawn();
                (endpoint, router)
            });
            Self {
                runtime,
                endpoint,
                _router: router,
            }
        }

        fn dialable(&self) -> Vec<SocketAddr> {
            usable_addrs(self.endpoint.bound_sockets())
        }
    }

    #[test]
    fn an_ack_for_a_different_connection_is_refused() {
        let alice = Identity::live("alice");
        let bob = Identity::live("bob");
        let alice_node = alice.node();
        alice_node.set_contacts(vec![bob.policy()]);
        let fake = FakePeer::open(&bob);
        let target = PeerTarget {
            fingerprint: bob.fingerprint(),
            endpoint_id: fake.endpoint.id().to_string(),
            addrs: fake.dialable(),
        };
        let error = alice_node
            .runtime
            .block_on(handle(&alice_node).deliver(&target, vec![Frame::Goodbye]))
            .unwrap_err();
        assert_eq!(error, PeerError::NonceMismatch);
        // A nonce that does not belong to this connection proves nothing, so the failed
        // handshake must leave no trace: no peer, no pin, no ingested frames.
        assert!(alice_node.peers().is_empty());
        assert!(alice_node.take_accepted().is_empty());
        assert!(alice_node.drain_inbox().is_empty());
    }

    #[test]
    fn a_notice_that_speaks_for_another_profile_closes_the_conversation() {
        let alice = Identity::live("alice");
        let bob = Identity::live("bob");
        let carol = Identity::live("carol");
        let alice_node = alice.node();
        alice_node.set_contacts(vec![bob.policy()]);
        let fake = FakePeer::open(&bob);
        let alice_addr = endpoint_addr(&alice_node.endpoint, dialable(&alice_node));
        fake.runtime.block_on(async {
            let connection = fake
                .endpoint
                .connect(alice_addr, PEER_ALPN)
                .await
                .expect("connect to Alice");
            let (mut send, mut recv) = connection.open_bi().await.expect("a stream");
            write_frame(
                &mut send,
                &Frame::Hello {
                    certificate: bob.certificate.clone(),
                    nonce: connection_nonce(),
                },
            )
            .await
            .expect("a hello");
            let ack = read_frame(&mut recv, MAX_FRAME_BYTES)
                .await
                .expect("a readable ack")
                .expect("an ack");
            assert!(matches!(ack, Frame::HelloAck { .. }));
            // This side authenticated as Bob, so a notice naming Carol can only be forged.
            let _ = write_frame(
                &mut send,
                &Frame::Notice {
                    kind: UpdateKind::Post,
                    fingerprint: carol.fingerprint(),
                },
            )
            .await;
            let _ = read_frame(&mut recv, MAX_FRAME_BYTES).await;
        });
        wait_for_error(&alice_node, "different profile");
        // The handshake itself succeeded, so Bob's device is kept as a pin; only the notice
        // that spoke for someone else was refused, and it never reached the inbox.
        assert_eq!(alice_node.take_accepted().len(), 1);
        assert!(alice_node.drain_inbox().is_empty());
    }
}
