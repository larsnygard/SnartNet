//! Desktop view model derived from daemon snapshots.
//!
//! The daemon decrypts for the authenticated local frontend and never sends
//! secret keys, so the desktop renders plaintext (or the daemon's decryption
//! error) instead of holding a keypair of its own.

use super::model::{
    Contact, DeliveryState, DeliveryStatus, DhtStatus, PeerStatus, RelayView, TorrentStatus,
};
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub(crate) struct NearbyPeer {
    pub fingerprint: String,
    pub alias: String,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NetworkView {
    /// Address peers are told to dial, as advertised by the daemon.
    pub listening: Option<String>,
    /// Optional operator override for the advertised address.
    pub address_override: String,
    pub peers: usize,
    pub dht: Option<DhtStatus>,
    pub torrent: Option<TorrentStatus>,
    /// Authenticated iroh peer endpoint (ADR 0003), replacing the old gossip topic.
    pub peer: Option<PeerStatus>,
    /// Durable publication state: whether this host can publish, why it last failed, and how
    /// much inbound is spooled before acknowledgement (M7.3/M7.5).
    pub delivery: DeliveryStatus,
    /// Relay selection and local relay health (M8).
    pub relay: RelayView,
    /// Scheduler mode the daemon is actually running, not what was requested.
    pub sync_mode: SyncMode,
    pub paused: bool,
    pub discovery: bool,
    pub last_sync: String,
    pub listener_error: Option<String>,
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
            address_override: string(extra, "address").unwrap_or_default(),
            peers: extra.get("peers").and_then(Value::as_u64).unwrap_or(0) as usize,
            dht: status(extra, "dht")?,
            torrent: status(extra, "torrent")?,
            peer: status(extra, "peer")?,
            delivery: delivery_status(extra),
            relay: relay_view(extra),
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
        self.contact(fingerprint).is_some_and(|contact| {
            contact.verification == super::model::VerificationState::Verified
                && contact.known_encryption_public_key.is_some()
        })
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

fn status<T: serde::de::DeserializeOwned>(
    extra: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<T>, String> {
    match extra.get(field) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|e| format!("unreadable {field} status in daemon snapshot: {e}")),
    }
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

/// The durable publication summary. A daemon that never published anything still reports the
/// object, so a window shows "no durable path" instead of inventing one.
fn delivery_status(extra: &serde_json::Map<String, Value>) -> DeliveryStatus {
    extra
        .get("delivery")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

/// The relay selection summary. Like the delivery block it is always present, so a window can
/// tell "n0 production relays" apart from a daemon that reported nothing at all.
fn relay_view(extra: &serde_json::Map<String, Value>) -> RelayView {
    extra
        .get("relay")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
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
        (None, Some(error)) => Err(error.to_owned()),
        (None, None) => Err("message text unavailable".into()),
    };
    Ok(MessageView {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .ok_or("daemon snapshot message without an id")?
            .to_owned(),
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
        delivery: value
            .get("delivery")
            .and_then(|delivery| serde_json::from_value(delivery.clone()).ok())
            .unwrap_or_default(),
        delivery_error: value
            .get("deliveryError")
            .and_then(Value::as_str)
            .map(str::to_owned),
        created_label: string_field(value, "time"),
        plaintext,
    })
}

fn nearby_from_value(value: &Value) -> NearbyPeer {
    NearbyPeer {
        fingerprint: string_field(value, "fingerprint"),
        alias: string_field(value, "alias"),
        address: value
            .get("address")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn string_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
