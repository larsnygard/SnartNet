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

/// Whether the daemon has handed a message to a peer or is still retrying.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeliveryState {
    #[default]
    Queued,
    Relayed,
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

/// Internet-wide peer discovery health, from the snapshot's `gossip` key.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GossipStatus {
    pub active: bool,
    pub node_id: Option<String>,
    pub peer_count: usize,
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
