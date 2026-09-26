//! Persisted chat models and ephemeral form state. Serde defaults preserve existing data.
use super::default_trust;
use serde::{Deserialize, Serialize};
use snartnet_core::{KeyPair, SignedMessage, SignedPost, SignedProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    Feed,
    Profile,
    Contacts,
    Messages,
    Network,
}

/// Which path is active in the "Add contact" section of the Contacts panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddContactMode {
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
pub enum VerificationState {
    #[default]
    Unknown,
    Verified,
    SignatureInvalid,
    FingerprintMismatch,
    MissingPeerProfile,
}

impl VerificationState {
    pub fn label(self) -> &'static str {
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
pub struct Contact {
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
    /// Newest device endpoint id this contact authenticated with (ADR 0003).
    #[serde(default)]
    pub peer_endpoint_id: Option<String>,
    /// Direct iroh addresses learned for that device (its UDP sockets, or the addresses a
    /// device descriptor carried). Distinct from `transport_addr`, which is the TCP sync
    /// port: dialing an endpoint on a TCP port was a bug that only ever worked by accident.
    #[serde(default)]
    pub peer_addrs: Vec<String>,
    /// `issued_at` of the newest accepted device certificate, so a replayed older one is refused.
    #[serde(default)]
    pub peer_certificate_issued_at: Option<u64>,
    #[serde(default)]
    pub last_sync_error: Option<String>,
    /// This contact's storage rule, which may only narrow what we host (M9.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_policy: Option<crate::replica::StoragePolicy>,
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
            peer_endpoint_id: None,
            peer_addrs: Vec::new(),
            peer_certificate_issued_at: None,
            last_sync_error: None,
            storage_policy: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatItem {
    pub id: String,
    pub incoming: bool,
    /// Stored payload; ciphertext for encrypted messages.
    pub content: String,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub encryption_alg: Option<String>,
    #[serde(default)]
    pub nonce_b64: Option<String>,
    pub pushed_via_bittorrent: bool,
    /// Set when this message arrived over a direct iroh chat connection rather than
    /// through the BitTorrent/DHT mailbox or an inbox pull.
    #[serde(default)]
    pub pushed_via_iroh: bool,
    pub created_label: String,
    #[serde(default)]
    pub verified_sender: bool,
    /// Keep the signed ciphertext for retry after restart; never persist the draft plaintext.
    #[serde(default)]
    pub envelope: Option<SignedMessage>,
    #[serde(default)]
    pub delivery: DeliveryState,
    /// Why the last durable publication of this outbound message failed (M7.1).
    ///
    /// A message with a reason here has no durable copy, so it is not pushed: the state and
    /// the reason are shown together instead of the message looking delivered.
    #[serde(default)]
    pub delivery_error: Option<String>,
    /// Preserve the key used for this message if a contact later rotates encryption keys.
    #[serde(default)]
    pub peer_encryption_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatThread {
    pub contact_fingerprint: String,
    pub messages: Vec<ChatItem>,
    #[serde(default)]
    pub unread_count: u32,
}

#[derive(Debug, Clone)]
pub struct NetworkState {
    pub bittorrent_running: bool,
    pub peers: u32,
    pub last_poll_label: String,
    pub poll_interval_secs: u64,
    pub lan_discovery_active: bool,
    pub discovered_peer_count: usize,
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
pub struct FormState {
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

#[derive(Debug, Clone)]
pub struct StartupData {
    pub keypair: Option<KeyPair>,
    pub profile: Option<SignedProfile>,
    pub local_posts: Vec<SignedPost>,
    pub contacts: Vec<Contact>,
    pub threads: Vec<ChatThread>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    /// Persisted locally, but no durable copy is published yet (M7.1).
    #[default]
    Queued,
    /// Published and addressable: a contact can fetch it without us being online.
    Available,
    /// A contact signed a storage receipt for its replica (M9).
    Stored,
    /// Pushed straight to the recipient over plain TCP or the iroh peer channel.
    Relayed,
    /// Inbound object we persisted and verified before acknowledging it (M7.3).
    Received,
}

impl DeliveryState {
    /// Whether a durable copy of this object exists somewhere other than our memory.
    pub fn is_durable(self) -> bool {
        matches!(
            self,
            DeliveryState::Available | DeliveryState::Stored | DeliveryState::Received
        )
    }

    /// Whether an outbound message in this state still needs a delivery attempt.
    pub fn is_pending(self) -> bool {
        matches!(self, DeliveryState::Queued | DeliveryState::Available)
    }

    /// The most durable state reached so far, so two paths cannot downgrade it.
    ///
    /// Ranked `Queued < Available < Stored < Relayed`, with `Received` only ever inbound.
    pub fn strongest(self, other: DeliveryState) -> DeliveryState {
        fn rank(state: DeliveryState) -> u8 {
            match state {
                DeliveryState::Queued => 0,
                DeliveryState::Available => 1,
                DeliveryState::Stored => 2,
                DeliveryState::Relayed => 3,
                DeliveryState::Received => 4,
            }
        }
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DeliveryState::Queued => "queued",
            DeliveryState::Available => "available",
            DeliveryState::Stored => "replica-stored",
            DeliveryState::Relayed => "relayed",
            DeliveryState::Received => "received",
        }
    }
}

impl ChatItem {
    pub fn from_signed(signed: SignedMessage, incoming: bool, peer_key: Option<String>) -> Self {
        let message = &signed.message;
        Self {
            id: message.id.clone(),
            incoming,
            content: message.content.clone(),
            encrypted: message.encrypted,
            encryption_alg: message.body_enc.clone(),
            nonce_b64: message.nonce_b64.clone(),
            pushed_via_bittorrent: false,
            pushed_via_iroh: false,
            created_label: message.created_at.format("%d %b · %H:%M UTC").to_string(),
            verified_sender: true,
            // Inbound means we already persisted it: that is what `Received` reports.
            delivery: if incoming {
                DeliveryState::Received
            } else {
                DeliveryState::Queued
            },
            delivery_error: None,
            peer_encryption_key: peer_key,
            envelope: Some(signed),
        }
    }

    /// Record one delivered path without downgrading an earlier, stronger state.
    pub fn record_path(&mut self, bittorrent: bool, iroh: bool) {
        self.pushed_via_bittorrent |= bittorrent;
        self.pushed_via_iroh |= iroh;
        if bittorrent || iroh {
            self.delivery = self.delivery.strongest(DeliveryState::Relayed);
        }
    }
}
