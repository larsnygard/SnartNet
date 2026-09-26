//! Terminal view models derived from daemon snapshots.
//!
//! These mirror exactly what the daemon serialises, so the terminal UI can render
//! daemon state without linking any client-side machinery (ADR 0001). The daemon
//! decrypts for the authenticated local frontend and never sends secret keys, so
//! the UI renders plaintext (or the daemon's decryption error) instead of holding
//! a keypair of its own.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use snartnet_core::{Profile, SignedPost};
use snartnet_sdk::{Snapshot, SyncMode};

#[derive(Debug, Clone, Default)]
pub(crate) struct DaemonState {
    pub profile: Option<Profile>,
    pub identity_uri: Option<String>,
    pub posts: Vec<SignedPost>,
    pub contacts: Vec<Contact>,
    pub threads: Vec<ThreadView>,
    pub nearby: Vec<NearbyPeer>,
    pub network: NetworkView,
}

/// Trust score the daemon assigns to contacts it has never traded with.
fn default_trust() -> u8 {
    20
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

/// How far a message has travelled. Mirrors the daemon's `DeliveryState` (M7.5).
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
            DeliveryState::Queued => "queued",
            DeliveryState::Available => "available",
            DeliveryState::Stored => "replica-stored",
            DeliveryState::Relayed => "relayed",
            DeliveryState::Received => "received",
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Contact {
    pub fingerprint: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub transport_addr: Option<String>,
    #[serde(default)]
    pub last_sync_label: String,
    #[serde(default)]
    pub profile_summary: String,
    #[serde(default)]
    pub latest_post_preview: String,
    #[serde(default)]
    pub verification: VerificationState,
    #[serde(default = "default_trust")]
    pub trust_score: u8,
    #[serde(default)]
    pub last_sync_error: Option<String>,
    #[serde(default)]
    pub known_encryption_public_key: Option<String>,
}

impl Contact {
    /// What to show in a list: the alias the user chose, else the fingerprint.
    pub(crate) fn label(&self) -> &str {
        if self.alias.is_empty() {
            &self.fingerprint
        } else {
            &self.alias
        }
    }

    /// True when the daemon holds a verified encryption key and can encrypt to it.
    pub(crate) fn ready_to_message(&self) -> bool {
        self.verification == VerificationState::Verified
            && self.known_encryption_public_key.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ThreadView {
    pub contact_fingerprint: String,
    pub unread_count: u32,
    pub messages: Vec<MessageView>,
}

#[derive(Debug, Clone)]
pub(crate) struct MessageView {
    pub id: String,
    pub incoming: bool,
    /// Stored payload: ciphertext for encrypted messages.
    pub ciphertext: String,
    pub encrypted: bool,
    pub delivery: DeliveryState,
    /// Why the daemon could not publish a durable copy of this message (M7.1).
    pub delivery_error: Option<String>,
    pub created_label: String,
    /// Daemon-side decryption result for this authenticated frontend.
    pub plaintext: Result<String, String>,
}

impl MessageView {
    /// The state a row shows, with the publication reason when there is one.
    pub(crate) fn state_label(&self) -> String {
        match &self.delivery_error {
            Some(error) => format!("{} ({error})", self.delivery.label()),
            None => self.delivery.label().to_string(),
        }
    }

    /// Body text, or the stored ciphertext when the user asks for the raw payload.
    pub(crate) fn body(&self, reveal_ciphertext: bool) -> String {
        if reveal_ciphertext && !self.ciphertext.is_empty() {
            return self.ciphertext.clone();
        }
        match &self.plaintext {
            Ok(text) => text.clone(),
            Err(error) => format!("[{error}]"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NearbyPeer {
    pub fingerprint: String,
    pub alias: String,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NetworkView {
    /// Address peers are told to dial, as advertised by the daemon.
    pub listening: Option<String>,
    /// Configured address the daemon reports.
    pub address: String,
    pub peers: usize,
    /// Scheduler mode the daemon is actually running, not what was requested.
    pub sync_mode: SyncMode,
    pub paused: bool,
    pub discovery: bool,
    pub last_sync: String,
    pub listener_error: Option<String>,
    /// Durable publication state: whether this host can publish, why it last failed, and how
    /// much inbound is spooled before acknowledgement (M7.3/M7.5).
    pub delivery: DeliveryStatus,
    /// DHT, torrent, and authenticated peer status objects, already summarised for display.
    pub subsystems: Vec<(&'static str, String)>,
}

impl DaemonState {
    pub(crate) fn from_snapshot(snapshot: &Snapshot) -> Result<Self, String> {
        let state = &snapshot.state;
        let profile = match &state.profile {
            Some(value) => Some(
                serde_json::from_value::<Profile>(value.clone())
                    .map_err(|e| format!("unreadable profile in daemon snapshot: {e}"))?,
            ),
            None => None,
        };
        let posts = values(&state.posts, "posts")?;
        let contacts = values(&state.contacts, "contacts")?;
        let threads = state
            .threads
            .iter()
            .map(thread_from_value)
            .collect::<Result<Vec<_>, _>>()?;
        let extra = &state.extra;
        let nearby = extra
            .get("nearby")
            .and_then(Value::as_array)
            .map(|peers| peers.iter().map(nearby_from_value).collect())
            .unwrap_or_default();
        let network = NetworkView {
            listening: string(extra, "listening"),
            address: string(extra, "address").unwrap_or_default(),
            peers: extra.get("peers").and_then(Value::as_u64).unwrap_or(0) as usize,
            sync_mode: sync_mode(extra)?,
            paused: extra
                .get("paused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            discovery: extra
                .get("discovery")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            last_sync: string(extra, "lastSync").unwrap_or_else(|| "never".into()),
            listener_error: string(extra, "listenerError"),
            delivery: extra
                .get("delivery")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            subsystems: ["dht", "torrent", "peer"]
                .into_iter()
                .filter_map(|name| {
                    extra
                        .get(name)
                        .filter(|value| !value.is_null())
                        .map(|value| (name, summarise(value)))
                })
                .collect(),
        };
        Ok(Self {
            profile,
            identity_uri: state.identity_uri.clone(),
            posts,
            contacts,
            threads,
            nearby,
            network,
        })
    }

    pub(crate) fn contact(&self, fingerprint: &str) -> Option<&Contact> {
        self.contacts
            .iter()
            .find(|contact| contact.fingerprint == fingerprint)
    }

    pub(crate) fn thread(&self, fingerprint: &str) -> Option<&ThreadView> {
        self.threads
            .iter()
            .find(|thread| thread.contact_fingerprint == fingerprint)
    }

    pub(crate) fn total_unread(&self) -> u32 {
        self.threads.iter().map(|thread| thread.unread_count).sum()
    }

    /// Contacts whose verified encryption key allows sending right now.
    pub(crate) fn is_ready_to_message(&self, fingerprint: &str) -> bool {
        self.contact(fingerprint)
            .is_some_and(Contact::ready_to_message)
    }

    /// A name for a fingerprint, so the UI never shows a bare 64-character key.
    pub(crate) fn contact_label(&self, fingerprint: &str) -> String {
        match self.contact(fingerprint) {
            Some(contact) => contact.label().to_string(),
            None => short(fingerprint),
        }
    }
}

/// Fingerprints are long; lists and status lines show their head only.
pub(crate) fn short(fingerprint: &str) -> String {
    let head: String = fingerprint.chars().take(12).collect();
    if head.len() < fingerprint.len() {
        format!("{head}…")
    } else {
        head
    }
}

fn summarise(value: &Value) -> String {
    match serde_json::to_string(value) {
        Ok(text) => text.trim_matches('"').to_string(),
        Err(_) => "unknown".into(),
    }
}

fn values<T: serde::de::DeserializeOwned>(values: &[Value], field: &str) -> Result<Vec<T>, String> {
    values
        .iter()
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|e| format!("unreadable {field} entry in daemon snapshot: {e}"))
        })
        .collect()
}

fn string(extra: &serde_json::Map<String, Value>, field: &str) -> Option<String> {
    extra
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The daemon injects its scheduler mode, because the session alone cannot know
/// whether cadence is always-on, balanced, or paused.
fn sync_mode(extra: &serde_json::Map<String, Value>) -> Result<SyncMode, String> {
    match extra.get("syncMode") {
        None => Ok(SyncMode::default()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|e| format!("unreadable syncMode in daemon snapshot: {e}")),
    }
}

fn thread_from_value(value: &Value) -> Result<ThreadView, String> {
    let contact_fingerprint = value
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or("daemon snapshot thread without a fingerprint")?
        .to_owned();
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .map(message_from_value)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(ThreadView {
        contact_fingerprint,
        unread_count: value.get("unread").and_then(Value::as_u64).unwrap_or(0) as u32,
        messages,
    })
}

fn message_from_value(value: &Value) -> Result<MessageView, String> {
    let plaintext = match (
        value.get("text").and_then(Value::as_str),
        value.get("error").and_then(Value::as_str),
    ) {
        (Some(text), _) => Ok(text.to_owned()),
        (None, Some(error)) => Err(format!("cannot decrypt: {error}")),
        (None, None) => Err("daemon sent no message body".to_owned()),
    };
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or("daemon snapshot message without an id")?
        .to_owned();
    Ok(MessageView {
        id,
        incoming: value
            .get("incoming")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ciphertext: value
            .get("ciphertext")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        encrypted: value
            .get("encrypted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        delivery: serde_json::from_value(value.get("delivery").cloned().unwrap_or(Value::Null))
            .unwrap_or_default(),
        delivery_error: value
            .get("deliveryError")
            .and_then(Value::as_str)
            .map(str::to_owned),
        created_label: value
            .get("time")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        plaintext,
    })
}

fn nearby_from_value(value: &Value) -> NearbyPeer {
    NearbyPeer {
        fingerprint: value
            .get("fingerprint")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        alias: value
            .get("alias")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        address: value
            .get("address")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}
