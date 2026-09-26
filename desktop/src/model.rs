//! Desktop view models.
//!
//! These mirror exactly what the daemon serialises into snapshots, so the
//! frontend can render daemon state without linking any of the client-side
//! machinery (ADR 0001). Nothing here reads a database, socket, or key file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Panel {
    #[default]
    Feed,
    Profile,
    Contacts,
    Messages,
    Network,
}

/// Which path is active in the "Add contact" section of the Contacts panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AddContactMode {
    /// Enter a fingerprint and alias manually (existing flow).
    Manual,
    /// Paste a base64 invite code copied from another user's profile panel.
    #[default]
    Invite,
    /// Paste a profile magnet URI copied from another user's profile panel.
    Magnet,
    /// Pick a peer that was discovered via LAN broadcast.
    LanPeer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationState {
    #[default]
    Unknown,
    Verified,
    SignatureInvalid,
    FingerprintMismatch,
    MissingPeerProfile,
}

impl VerificationState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            VerificationState::Unknown => "unknown",
            VerificationState::Verified => "verified",
            VerificationState::SignatureInvalid => "signature-invalid",
            VerificationState::FingerprintMismatch => "fingerprint-mismatch",
            VerificationState::MissingPeerProfile => "missing-profile",
        }
    }
}

/// Trust score the daemon assigns to contacts it has never traded with.
fn default_trust() -> u8 {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Contact {
    pub fingerprint: String,
    pub alias: String,
    pub magnet_uri: Option<String>,
    #[serde(default)]
    pub transport_addr: Option<String>,
    #[serde(default)]
    pub avatar_data_url: Option<String>,
    pub auto_synced: bool,
    pub last_sync_label: String,
    pub profile_summary: String,
    pub latest_post_preview: String,
    #[serde(default)]
    pub verification: VerificationState,
    #[serde(default = "default_trust")]
    pub trust_score: u8,
    #[serde(default)]
    pub synced_post_count: usize,
    #[serde(default)]
    pub known_public_key: Option<String>,
    #[serde(default)]
    pub known_encryption_public_key: Option<String>,
    #[serde(default)]
    pub last_sync_error: Option<String>,
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            fingerprint: String::new(),
            alias: String::new(),
            magnet_uri: None,
            transport_addr: None,
            avatar_data_url: None,
            auto_synced: false,
            last_sync_label: String::new(),
            profile_summary: String::new(),
            latest_post_preview: String::new(),
            verification: VerificationState::Unknown,
            trust_score: default_trust(),
            synced_post_count: 0,
            known_public_key: None,
            known_encryption_public_key: None,
            last_sync_error: None,
        }
    }
}

/// How far a message has travelled. Mirrors the daemon's `DeliveryState` (M7.5).
///
/// The five states are the ones delivery can actually reach, so the window cannot show a
/// sixth state the daemon never sets. `Stored` is a contact's replica receipt (M9).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeliveryState {
    /// Persisted locally, but no durable copy is published yet.
    #[default]
    Queued,
    /// Published and addressable: the recipient can fetch it without us being online.
    Available,
    /// A contact signed a storage receipt for its replica (M9).
    Stored,
    /// Handed to the recipient over the torrent or iroh path.
    Relayed,
    /// An inbound object the daemon stored and verified before acknowledging it.
    Received,
}

impl DeliveryState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            DeliveryState::Queued => "Queued",
            DeliveryState::Available => "Available",
            DeliveryState::Stored => "Replica stored",
            DeliveryState::Relayed => "Relayed",
            DeliveryState::Received => "Received",
        }
    }

    /// Whether this message is still waiting for a hand-off or a durable copy.
    pub(crate) fn is_pending(self) -> bool {
        matches!(self, DeliveryState::Queued)
    }
}

/// What the durable publication path is doing, from the snapshot's `delivery` key (M7.5).
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct DeliveryStatus {
    /// Whether this host can publish a durable copy at all.
    #[serde(default)]
    pub durable: bool,
    /// Why the last publication failed, when it did.
    #[serde(default)]
    pub failed: Option<String>,
    /// Objects spooled before acknowledgement and not yet folded into state (M7.3).
    #[serde(default)]
    pub spooled: u64,
}

/// How relays were chosen and how they behave, from the snapshot's `relay` key (M8).
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RelayView {
    /// One line naming the source and the relays in the plan.
    #[serde(default)]
    pub plan: String,
    /// Which source decided the plan (`configured`, `referral`, `community`, or `n0`).
    #[serde(default)]
    pub source: Option<String>,
    /// Whether relaying is switched off entirely.
    #[serde(default)]
    pub disabled: bool,
    /// The relay URLs this client applied to its endpoint.
    #[serde(default)]
    pub active: Vec<String>,
    /// Local observations, best first (M8.4).
    #[serde(default)]
    pub health: Vec<RelayHealthView>,
}

/// One relay's local health, as the daemon scored it.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RelayHealthView {
    pub url: String,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FormState {
    pub username_input: String,
    pub display_name_input: String,
    pub bio_input: String,
    pub avatar_path_input: String,
    pub avatar_data_url: Option<String>,
    pub contact_fingerprint_input: String,
    pub contact_alias_input: String,
    pub compose_post_input: String,
    pub compose_message_input: String,
    pub selected_contact_for_chat: Option<String>,
    /// Add-contact mode selector in the Contacts panel.
    pub add_contact_mode: AddContactMode,
    /// Invite code string pasted by the user.
    pub invite_code_input: String,
    /// Profile magnet URI pasted by the user.
    pub magnet_uri_input: String,
    pub search: String,
    pub qr_path: String,
    pub advertise_addr: String,
    /// Drafts belong to conversations, so switching contacts never sends text to the wrong person.
    pub drafts: std::collections::HashMap<String, String>,
}

/// DHT health as reported by the daemon in the snapshot's `dht` key.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DhtStatus {
    pub bootstrapped: bool,
    pub last_lookup: Option<String>,
    pub last_publish: Option<String>,
    pub last_error: Option<String>,
}

/// Internet-wide peer connectivity, from the snapshot's `peer` key.
///
/// Fields mirror `snartnet_client::peer::PeerStatus`: `node_id` is the device endpoint id
/// from ADR 0003, not the profile key.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PeerStatus {
    pub active: bool,
    pub node_id: Option<String>,
    pub peer_count: usize,
    #[serde(default)]
    pub discovery: String,
    pub last_error: Option<String>,
}

/// Torrent session health, from the snapshot's `torrent` key.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TorrentStatus {
    pub listening: bool,
    pub reachability: String,
    pub peer_count: u64,
    pub last_fetch: Option<String>,
    pub last_publish: Option<String>,
    pub last_error: Option<String>,
}
