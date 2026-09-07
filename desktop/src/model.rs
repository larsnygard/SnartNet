//! Persisted chat models and ephemeral form state. Serde defaults preserve existing data.
use super::default_trust;
use serde::{Deserialize, Serialize};
use snartnet_core::{KeyPair, SignedMessage, SignedPost, SignedProfile};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Contact {
    pub(crate) fingerprint: String,
    pub(crate) alias: String,
    pub(crate) magnet_uri: Option<String>,
    #[serde(default)]
    pub(crate) transport_addr: Option<String>,
    #[serde(default)]
    pub(crate) avatar_data_url: Option<String>,
    pub(crate) auto_synced: bool,
    pub(crate) last_sync_label: String,
    pub(crate) profile_summary: String,
    pub(crate) latest_post_preview: String,
    #[serde(default)]
    pub(crate) verification: VerificationState,
    #[serde(default = "default_trust")]
    pub(crate) trust_score: u8,
    #[serde(default)]
    pub(crate) synced_post_count: usize,
    #[serde(default)]
    pub(crate) known_public_key: Option<String>,
    #[serde(default)]
    pub(crate) known_encryption_public_key: Option<String>,
    #[serde(default)]
    pub(crate) last_sync_error: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatItem {
    pub(crate) id: String,
    pub(crate) incoming: bool,
    /// Stored payload; ciphertext for encrypted messages.
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) encrypted: bool,
    #[serde(default)]
    pub(crate) encryption_alg: Option<String>,
    #[serde(default)]
    pub(crate) nonce_b64: Option<String>,
    pub(crate) pushed_via_bittorrent: bool,
    pub(crate) created_label: String,
    #[serde(default)]
    pub(crate) verified_sender: bool,
    /// Keep the signed ciphertext for retry after restart; never persist the draft plaintext.
    #[serde(default)]
    pub(crate) envelope: Option<SignedMessage>,
    #[serde(default)]
    pub(crate) delivery: DeliveryState,
    /// Preserve the key used for this message if a contact later rotates encryption keys.
    #[serde(default)]
    pub(crate) peer_encryption_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatThread {
    pub(crate) contact_fingerprint: String,
    pub(crate) messages: Vec<ChatItem>,
    #[serde(default)]
    pub(crate) unread_count: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct NetworkState {
    pub(crate) bittorrent_running: bool,
    pub(crate) peers: u32,
    pub(crate) last_poll_label: String,
    pub(crate) poll_interval_secs: u64,
    pub(crate) lan_discovery_active: bool,
    pub(crate) discovered_peer_count: usize,
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            bittorrent_running: true,
            peers: 0,
            last_poll_label: "never".to_string(),
            poll_interval_secs: 4,
            lan_discovery_active: false,
            discovered_peer_count: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FormState {
    pub(crate) username_input: String,
    pub(crate) display_name_input: String,
    pub(crate) bio_input: String,
    pub(crate) avatar_path_input: String,
    pub(crate) avatar_data_url: Option<String>,
    pub(crate) contact_fingerprint_input: String,
    pub(crate) contact_alias_input: String,
    pub(crate) compose_post_input: String,
    pub(crate) compose_message_input: String,
    pub(crate) selected_contact_for_chat: Option<String>,
    /// Add-contact mode selector in the Contacts panel.
    pub(crate) add_contact_mode: AddContactMode,
    /// Invite code string pasted by the user.
    pub(crate) invite_code_input: String,
    /// Profile magnet URI pasted by the user.
    pub(crate) magnet_uri_input: String,
    pub(crate) search: String,
    pub(crate) qr_path: String,
    pub(crate) advertise_addr: String,
    /// Drafts belong to conversations, so switching contacts never sends text to the wrong person.
    pub(crate) drafts: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StartupData {
    pub(crate) keypair: Option<KeyPair>,
    pub(crate) profile: Option<SignedProfile>,
    pub(crate) local_posts: Vec<SignedPost>,
    pub(crate) contacts: Vec<Contact>,
    pub(crate) threads: Vec<ChatThread>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeliveryState {
    #[default]
    Queued,
    Relayed,
}

impl ChatItem {
    pub(crate) fn from_signed(
        signed: SignedMessage,
        incoming: bool,
        peer_key: Option<String>,
    ) -> Self {
        let message = &signed.message;
        Self {
            id: message.id.clone(),
            incoming,
            content: message.content.clone(),
            encrypted: message.encrypted,
            encryption_alg: message.body_enc.clone(),
            nonce_b64: message.nonce_b64.clone(),
            pushed_via_bittorrent: false,
            created_label: message.created_at.format("%d %b · %H:%M UTC").to_string(),
            verified_sender: true,
            delivery: DeliveryState::Queued,
            peer_encryption_key: peer_key,
            envelope: Some(signed),
        }
    }
}
