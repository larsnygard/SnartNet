//! Contact replication and storage policy (M9).
//!
//! A device can hold a copy of a contact's published objects so those objects outlive the
//! author's uptime. The rules are local, in this order:
//!
//! 1. a platform default ([`StorageSettings::for_platform`]): a desktop volunteers space, a
//!    phone does not volunteer and keeps a much smaller budget (M10 tightens this);
//! 2. the user's saved overrides (`storage_policy` in canonical state, set through the
//!    `storage` command);
//! 3. the deployment's overrides from the environment (`SNARTNET_STORAGE_*`), because an
//!    operator deploying a node has the last word;
//! 4. a per-contact rule, which can only *narrow* what we hold for that contact.
//!
//! A host only stores what it was asked to store in writing: a [`ReplicaLease`] whose payload
//! (object id, kind, size, source) is encrypted to the host, so a lease does not leak what it
//! asks for, and whose envelope is signed by the owner, so a host can prove who asked. What
//! the host returns is a [`StorageReceipt`] — its own signed promise that it holds the object
//! until a stated time — which is what moves the owner's delivery state to `replica-stored`
//! (M7.5). Neither record lets a host read what it stores: mailbox objects are opaque
//! ciphertext, and profiles and feeds are public but signed.
//!
//! Space is bounded locally and honestly (M9.4): admission refuses a replica that would cross
//! the quota or eat the free-space headroom, and [`eviction_plan`] decides what to drop first
//! — expired leases before the oldest live ones — so a host never fills a disk to obey a lease.
use serde::{Deserialize, Serialize};
use snartnet_core::{verify_signature, KeyPair};
use std::time::{SystemTime, UNIX_EPOCH};

/// Version of the lease and receipt format.
pub const REPLICA_PROTOCOL_VERSION: u8 = 1;

/// Largest object a host accepts as a replica, regardless of quota.
///
/// A lease is a request, not a command: an object larger than this is refused even on a host
/// with room to spare, so one contact cannot turn a replica host into bulk storage.
pub const MAX_REPLICA_BYTES: u64 = 64 * 1024 * 1024;

/// Most leases a host keeps, so the state stays bounded even if contacts ask constantly.
pub const MAX_HELD_LEASES: usize = 64;

/// Most leases one owner issues, so a long-lived identity cannot grow its state without limit.
pub const MAX_ISSUED_LEASES: usize = 64;

/// Most receipts kept, newest first.
pub const MAX_RECEIPTS: usize = 64;

/// Clock skew allowed on a lease or receipt timestamp, so two machines that disagree by a few
/// minutes still accept a record that was just issued.
pub const LEASE_CLOCK_SKEW_SECS: u64 = 300;

/// Environment variable naming the platform when it is not the desktop default.
pub const ENV_PLATFORM: &str = "SNARTNET_PLATFORM";

/// Environment variable that switches replica hosting on or off (`on`/`off`).
pub const ENV_STORAGE_REPLICATE: &str = "SNARTNET_STORAGE_REPLICATE";

/// Environment variable for the replica quota, in MiB.
pub const ENV_STORAGE_QUOTA_MB: &str = "SNARTNET_STORAGE_QUOTA_MB";

/// Environment variable for the lease lifetime, in days.
pub const ENV_STORAGE_LEASE_DAYS: &str = "SNARTNET_STORAGE_LEASE_DAYS";

/// Environment variable for the free-space headroom, in MiB.
pub const ENV_STORAGE_MIN_FREE_MB: &str = "SNARTNET_STORAGE_MIN_FREE_MB";

/// Environment variable for how many copies of our own objects we ask contacts to hold.
pub const ENV_STORAGE_COPIES: &str = "SNARTNET_STORAGE_COPIES";

/// Current unix time in seconds.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Which platform a default is chosen for (M9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// A machine that is usually online and has a disk: it volunteers space.
    Desktop,
    /// A phone: it does not volunteer, and its budget is small (M10).
    Mobile,
}

impl Platform {
    /// The platform this process is running as, from `SNARTNET_PLATFORM` when set.
    ///
    /// The Android bridge sets this rather than relying on a compile-time target check, so a
    /// desktop build used as a mobile test host behaves like the phone it stands in for.
    pub fn from_env() -> Self {
        match std::env::var(ENV_PLATFORM)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("mobile" | "android" | "phone") => Platform::Mobile,
            _ => Platform::Desktop,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Desktop => "desktop",
            Platform::Mobile => "mobile",
        }
    }
}

/// The concrete storage settings a host runs with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageSettings {
    /// Whether this host holds replicas for contacts at all.
    pub replicate: bool,
    /// Bytes this host dedicates to replicas.
    pub quota_bytes: u64,
    /// How long a replica is accepted for before it has to be renewed.
    pub lease_secs: u64,
    /// How many replica receipts this host asks contacts for its own objects.
    pub copies: u8,
    /// Free space that must remain on the volume, untouched by replicas.
    pub min_free_bytes: u64,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self::for_platform(Platform::Desktop)
    }
}

impl StorageSettings {
    /// The default for a platform (M9.1).
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::Desktop => Self {
                replicate: true,
                quota_bytes: 1024 * 1024 * 1024,
                lease_secs: 30 * 24 * 60 * 60,
                copies: 2,
                min_free_bytes: 512 * 1024 * 1024,
            },
            Platform::Mobile => Self {
                replicate: false,
                quota_bytes: 64 * 1024 * 1024,
                lease_secs: 7 * 24 * 60 * 60,
                copies: 1,
                min_free_bytes: 256 * 1024 * 1024,
            },
        }
    }

    /// The effective settings: platform default, then saved user overrides, then the
    /// deployment's environment overrides, then any per-contact narrowing (M9.1).
    pub fn resolve(
        platform: Platform,
        saved: &StoragePolicy,
        contact: Option<&StoragePolicy>,
    ) -> Self {
        let mut settings = Self::for_platform(platform);
        settings.apply(saved);
        settings.apply(&StoragePolicy::from_env());
        if let Some(contact) = contact {
            settings.apply_narrowing(contact);
        }
        settings.clamp()
    }

    /// Apply overrides: every field they set wins (the caller decides the precedence order).
    pub fn apply(&mut self, policy: &StoragePolicy) {
        if let Some(value) = policy.replicate {
            self.replicate = value;
        }
        if let Some(value) = policy.quota_bytes {
            self.quota_bytes = value;
        }
        if let Some(value) = policy.lease_secs {
            self.lease_secs = value;
        }
        if let Some(value) = policy.copies {
            self.copies = value;
        }
        if let Some(value) = policy.min_free_bytes {
            self.min_free_bytes = value;
        }
    }

    /// Apply a per-contact rule, which may only narrow what we already allow.
    pub fn apply_narrowing(&mut self, policy: &StoragePolicy) {
        if let Some(false) = policy.replicate {
            self.replicate = false;
        }
        if let Some(value) = policy.quota_bytes {
            self.quota_bytes = self.quota_bytes.min(value);
        }
        if let Some(value) = policy.lease_secs {
            self.lease_secs = self.lease_secs.min(value);
        }
        if let Some(value) = policy.min_free_bytes {
            self.min_free_bytes = self.min_free_bytes.max(value);
        }
    }

    /// Keep the settings usable: a zero quota or a zero lease would make replication a trap.
    pub fn clamp(mut self) -> Self {
        self.quota_bytes = self
            .quota_bytes
            .clamp(16 * 1024 * 1024, 1024 * 1024 * 1024 * 1024);
        self.lease_secs = self.lease_secs.clamp(60 * 60, 365 * 24 * 60 * 60);
        self.copies = self.copies.clamp(1, 5);
        self
    }

    /// One line for the snapshot and the storage panel.
    pub fn summary(&self) -> String {
        format!(
            "{} · quota {} MiB · lease {} d · {} copy/copies · headroom {} MiB",
            if self.replicate {
                "hosting"
            } else {
                "not hosting"
            },
            self.quota_bytes / (1024 * 1024),
            self.lease_secs / (24 * 60 * 60),
            self.copies,
            self.min_free_bytes / (1024 * 1024)
        )
    }
}

/// Overrides a user, a deployment, or a contact rule may set. Every field is optional, so an
/// override only speaks about what it actually changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoragePolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copies: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_free_bytes: Option<u64>,
}

impl StoragePolicy {
    /// Whether this policy changes anything.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The deployment's overrides from the environment.
    ///
    /// A malformed value is ignored rather than fatal: a typo in a container environment must
    /// not stop a node from starting, it just leaves that field at its previous value.
    pub fn from_env() -> Self {
        let env = |name: &str| std::env::var(name).ok();
        let mebibytes = |name: &str| {
            env(name)
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(|value| value * 1024 * 1024)
        };
        Self {
            replicate: env(ENV_STORAGE_REPLICATE).and_then(|value| match value.trim() {
                "on" | "true" | "yes" | "1" => Some(true),
                "off" | "false" | "no" | "0" => Some(false),
                _ => None,
            }),
            quota_bytes: mebibytes(ENV_STORAGE_QUOTA_MB),
            lease_secs: env(ENV_STORAGE_LEASE_DAYS)
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(|days| days * 24 * 60 * 60),
            copies: env(ENV_STORAGE_COPIES).and_then(|value| value.trim().parse::<u8>().ok()),
            min_free_bytes: mebibytes(ENV_STORAGE_MIN_FREE_MB),
        }
    }
}

/// What kind of object a lease covers (M9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicaKind {
    /// A signed profile.
    Profile,
    /// An author's signed feed snapshot.
    Feed,
    /// An encrypted mailbox object. Opaque to the host that holds it.
    Mailbox,
}

impl ReplicaKind {
    /// The object-id prefix the durable path publishes this kind under.
    pub fn prefix(self) -> &'static str {
        match self {
            ReplicaKind::Profile => "profile",
            ReplicaKind::Feed => "feed",
            ReplicaKind::Mailbox => "mailbox",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReplicaKind::Profile => "profile",
            ReplicaKind::Feed => "feed",
            ReplicaKind::Mailbox => "mailbox",
        }
    }
}

/// Why a lease, a receipt, or a replica was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplicaError {
    /// The record carries a format version this build does not understand.
    Version(u8),
    /// A required field is empty.
    Empty(String),
    /// The signature does not match the claimed author's key.
    Signature,
    /// The record is not valid yet.
    NotYetValid { issued_at: u64, now: u64 },
    /// The record has expired.
    Expired { expires_at: u64, now: u64 },
    /// A lease was addressed to a different host.
    NotHost { asked: String, host: String },
    /// The object is larger than one replica may be.
    TooLarge { bytes: u64, cap: u64 },
    /// This host does not host replicas.
    NotHosting,
    /// The replica would cross the configured quota.
    Quota {
        used: u64,
        requested: u64,
        quota: u64,
    },
    /// The replica would eat the free-space headroom.
    LowDisk { free: u64, headroom: u64 },
    /// A receipt names a lease this host never issued.
    UnknownLease(String),
}

impl std::fmt::Display for ReplicaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicaError::Version(version) => {
                write!(f, "unsupported replica protocol version {version}")
            }
            ReplicaError::Empty(field) => write!(f, "replica record without {field}"),
            ReplicaError::Signature => write!(f, "replica record signature is invalid"),
            ReplicaError::NotYetValid { issued_at, now } => write!(
                f,
                "replica record issued at {issued_at} is ahead of the local clock ({now})"
            ),
            ReplicaError::Expired { expires_at, now } => {
                write!(f, "replica record expired at {expires_at} (now {now})")
            }
            ReplicaError::NotHost { asked, host } => {
                write!(f, "replica lease is addressed to {asked}, not {host}")
            }
            ReplicaError::TooLarge { bytes, cap } => {
                write!(f, "replica of {bytes} bytes exceeds the {cap} byte cap")
            }
            ReplicaError::NotHosting => write!(f, "this host does not host replicas"),
            ReplicaError::Quota {
                used,
                requested,
                quota,
            } => write!(
                f,
                "replica would use {} of {quota} bytes (already {used})",
                used + requested
            ),
            ReplicaError::LowDisk { free, headroom } => write!(
                f,
                "only {free} bytes free, below the {headroom} byte headroom"
            ),
            ReplicaError::UnknownLease(lease) => {
                write!(f, "receipt names unknown lease {lease}")
            }
        }
    }
}

impl std::error::Error for ReplicaError {}

/// What a lease asks a host to store, sealed to that host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeasePayload {
    /// The object id the durable path publishes this object under.
    pub object_id: String,
    pub kind: ReplicaKind,
    /// Magnet the host can fetch the object from when it does not have it locally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnet: Option<String>,
    /// Size in bytes, on the author's word. The host re-checks it after fetching.
    pub bytes: u64,
}

/// The encrypted part of a lease: what to store, readable only by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedPayload {
    /// Encryption construction, so an old lease stays readable after an algorithm change.
    pub alg: String,
    /// base64 X25519 nonce.
    pub nonce_b64: String,
    /// base64 ChaCha20-Poly1305 ciphertext of the payload JSON.
    pub payload_enc: String,
}

impl SealedPayload {
    /// Seal a payload so only the host's own key can read it.
    pub fn seal(
        keypair: &KeyPair,
        host_encryption_public_key: &str,
        payload: &LeasePayload,
    ) -> Result<Self, String> {
        let json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
        let (payload_enc, nonce_b64, alg) =
            keypair.encrypt_for_recipient(host_encryption_public_key, &json)?;
        Ok(Self {
            alg,
            nonce_b64,
            payload_enc,
        })
    }

    /// Open the payload with the host's key and the owner's encryption key.
    pub fn open(
        &self,
        keypair: &KeyPair,
        owner_encryption_public_key: &str,
    ) -> Result<LeasePayload, String> {
        let json = keypair.decrypt_from_peer(
            owner_encryption_public_key,
            &self.nonce_b64,
            &self.payload_enc,
        )?;
        serde_json::from_str(&json).map_err(|e| format!("unreadable lease payload: {e}"))
    }
}

/// A request to hold one object until `expires_at` (M9.2).
///
/// The envelope is public and signed by the owner, so a host can prove *who* asked and check
/// the window before doing any work; the payload is sealed to the host, so the lease does not
/// tell anyone else what the owner stores where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaLease {
    pub v: u8,
    /// Identifies this request, so a receipt can name it and a resend cannot double-count.
    pub lease_id: String,
    /// Profile fingerprint of the owner asking for the replica.
    pub owner: String,
    /// Profile fingerprint of the host being asked.
    pub host: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub sealed: SealedPayload,
    /// base64 profile-key signature over [`ReplicaLease::signed_body`].
    pub signature: String,
}

/// The bytes a lease's signature covers.
#[derive(Debug, Serialize)]
struct LeaseBody<'a> {
    v: u8,
    lease_id: &'a str,
    owner: &'a str,
    host: &'a str,
    issued_at: u64,
    expires_at: u64,
    seal_hash: String,
}

impl ReplicaLease {
    /// Ask `host` to hold one object for `lease_secs` (M9.2).
    pub fn issue(
        keypair: &KeyPair,
        host_fingerprint: &str,
        host_encryption_public_key: &str,
        payload: &LeasePayload,
        now: u64,
        lease_secs: u64,
    ) -> Result<Self, String> {
        if host_fingerprint.trim().is_empty() {
            return Err("a lease needs a host".into());
        }
        if payload.object_id.trim().is_empty() {
            return Err("a lease needs an object".into());
        }
        let sealed = SealedPayload::seal(keypair, host_encryption_public_key, payload)?;
        let mut lease = Self {
            v: REPLICA_PROTOCOL_VERSION,
            lease_id: lease_id_for(
                &keypair.fingerprint,
                host_fingerprint,
                &payload.object_id,
                now,
            ),
            owner: keypair.fingerprint.clone(),
            host: host_fingerprint.to_owned(),
            issued_at: now,
            expires_at: now.saturating_add(lease_secs),
            sealed,
            signature: String::new(),
        };
        let body = lease.signed_body()?;
        lease.signature = keypair.sign(&body)?;
        Ok(lease)
    }

    /// The canonical string the signature covers.
    fn signed_body(&self) -> Result<String, String> {
        let seal_hash = blake3::hash(&serde_json::to_vec(&self.sealed).map_err(|e| e.to_string())?)
            .to_hex()
            .to_string();
        serde_json::to_string(&LeaseBody {
            v: self.v,
            lease_id: &self.lease_id,
            owner: &self.owner,
            host: &self.host,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            seal_hash,
        })
        .map_err(|e| e.to_string())
    }

    /// Verify the envelope against the owner's key and this host.
    ///
    /// The payload is deliberately *not* opened here: the host checks that the request is
    /// well-formed, signed by a contact it knows, addressed to itself, and inside its window
    /// before it spends a decryption or any disk space.
    pub fn verify(&self, owner_public_key: &str, host: &str, now: u64) -> Result<(), ReplicaError> {
        if self.v != REPLICA_PROTOCOL_VERSION {
            return Err(ReplicaError::Version(self.v));
        }
        if self.lease_id.trim().is_empty() {
            return Err(ReplicaError::Empty("lease id".into()));
        }
        if self.owner.trim().is_empty() {
            return Err(ReplicaError::Empty("owner".into()));
        }
        if self.host != host {
            return Err(ReplicaError::NotHost {
                asked: self.host.clone(),
                host: host.to_owned(),
            });
        }
        if self.issued_at > now + LEASE_CLOCK_SKEW_SECS {
            return Err(ReplicaError::NotYetValid {
                issued_at: self.issued_at,
                now,
            });
        }
        if self.expires_at <= now {
            return Err(ReplicaError::Expired {
                expires_at: self.expires_at,
                now,
            });
        }
        let body = self.signed_body().map_err(ReplicaError::Empty)?;
        let valid = verify_signature(&body, &self.signature, owner_public_key)
            .map_err(|_| ReplicaError::Signature)?;
        if !valid {
            return Err(ReplicaError::Signature);
        }
        Ok(())
    }

    /// Open the sealed payload with this host's key and the owner's encryption key.
    pub fn open_payload(
        &self,
        keypair: &KeyPair,
        owner_encryption_public_key: &str,
    ) -> Result<LeasePayload, String> {
        self.sealed.open(keypair, owner_encryption_public_key)
    }

    /// Whether this lease is still inside its window.
    pub fn is_active(&self, now: u64) -> bool {
        self.expires_at > now
    }
}

/// The id of a lease, derived from what it asks for so a resend of the same request reuses it.
pub fn lease_id_for(owner: &str, host: &str, object_id: &str, issued_at: u64) -> String {
    let digest = blake3::hash(format!("{owner}\n{host}\n{object_id}\n{issued_at}").as_bytes())
        .to_hex()
        .to_string();
    format!("lease-{}", &digest[..32])
}

/// A host's signed promise that it holds one object until `expires_at` (M9.2).
///
/// The receipt is public: it contains nothing secret, and it is the evidence the owner keeps
/// (and can show) that a copy exists somewhere other than its own disk. It is signed with the
/// host's profile key, so it cannot be forged by the owner or by a third party.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageReceipt {
    pub v: u8,
    /// The lease this receipt answers.
    pub lease_id: String,
    pub owner: String,
    pub host: String,
    pub object_id: String,
    pub kind: ReplicaKind,
    pub bytes: u64,
    pub stored_at: u64,
    pub expires_at: u64,
    /// base64 profile-key signature over [`StorageReceipt::signed_body`].
    pub signature: String,
}

/// The bytes a receipt's signature covers.
#[derive(Debug, Serialize)]
struct ReceiptBody<'a> {
    v: u8,
    lease_id: &'a str,
    owner: &'a str,
    host: &'a str,
    object_id: &'a str,
    kind: ReplicaKind,
    bytes: u64,
    stored_at: u64,
    expires_at: u64,
}

impl StorageReceipt {
    /// Issue a receipt for a lease whose object this host now holds.
    ///
    /// The lease is identified by its id, owner, and expiry rather than by the lease record
    /// itself: a host that stored the bytes before a restart can still answer from its own
    /// state, which is what makes a receipt survivable.
    pub fn issue(
        keypair: &KeyPair,
        lease_id: &str,
        owner: &str,
        expires_at: u64,
        payload: &LeasePayload,
        bytes: u64,
        stored_at: u64,
    ) -> Result<Self, String> {
        let mut receipt = Self {
            v: REPLICA_PROTOCOL_VERSION,
            lease_id: lease_id.to_owned(),
            owner: owner.to_owned(),
            host: keypair.fingerprint.clone(),
            object_id: payload.object_id.clone(),
            kind: payload.kind,
            bytes,
            stored_at,
            // A receipt never outlives the lease it answers.
            expires_at,
            signature: String::new(),
        };
        let body = receipt.signed_body()?;
        receipt.signature = keypair.sign(&body)?;
        Ok(receipt)
    }

    /// The canonical string the signature covers.
    fn signed_body(&self) -> Result<String, String> {
        serde_json::to_string(&ReceiptBody {
            v: self.v,
            lease_id: &self.lease_id,
            owner: &self.owner,
            host: &self.host,
            object_id: &self.object_id,
            kind: self.kind,
            bytes: self.bytes,
            stored_at: self.stored_at,
            expires_at: self.expires_at,
        })
        .map_err(|e| e.to_string())
    }

    /// Verify a receipt against the host's profile key.
    pub fn verify(&self, host_public_key: &str, now: u64) -> Result<(), ReplicaError> {
        if self.v != REPLICA_PROTOCOL_VERSION {
            return Err(ReplicaError::Version(self.v));
        }
        if self.lease_id.trim().is_empty() {
            return Err(ReplicaError::Empty("lease id".into()));
        }
        if self.host.trim().is_empty() {
            return Err(ReplicaError::Empty("host".into()));
        }
        if self.object_id.trim().is_empty() {
            return Err(ReplicaError::Empty("object id".into()));
        }
        if self.stored_at > now + LEASE_CLOCK_SKEW_SECS {
            return Err(ReplicaError::NotYetValid {
                issued_at: self.stored_at,
                now,
            });
        }
        // An expired receipt is history, not evidence: the copy may be gone with it.
        if self.expires_at <= now {
            return Err(ReplicaError::Expired {
                expires_at: self.expires_at,
                now,
            });
        }
        let body = self.signed_body().map_err(ReplicaError::Empty)?;
        let valid = verify_signature(&body, &self.signature, host_public_key)
            .map_err(|_| ReplicaError::Signature)?;
        if !valid {
            return Err(ReplicaError::Signature);
        }
        Ok(())
    }

    /// Whether this receipt still claims a live copy.
    pub fn is_active(&self, now: u64) -> bool {
        self.expires_at > now
    }
}

/// Frame payload key under which a peer channel object carries a replica lease (M9.2).
pub const REPLICA_LEASE_KEY: &str = "replica_lease";

/// Frame payload key under which a peer channel object carries a storage receipt (M9.2).
pub const REPLICA_RECEIPT_KEY: &str = "replica_receipt";

/// Wrap a lease for the peer channel.
pub fn lease_object(lease: &ReplicaLease) -> serde_json::Value {
    serde_json::json!({ REPLICA_LEASE_KEY: lease })
}

/// Whether an object is a replica lease, and if so which one.
pub fn lease_from_object(object: &serde_json::Value) -> Option<ReplicaLease> {
    serde_json::from_value(object.get(REPLICA_LEASE_KEY)?.clone()).ok()
}

/// Wrap a receipt for the peer channel.
pub fn receipt_object(receipt: &StorageReceipt) -> serde_json::Value {
    serde_json::json!({ REPLICA_RECEIPT_KEY: receipt })
}

/// Whether an object is a storage receipt, and if so which one.
pub fn receipt_from_object(object: &serde_json::Value) -> Option<StorageReceipt> {
    serde_json::from_value(object.get(REPLICA_RECEIPT_KEY)?.clone()).ok()
}

/// One replica this host accepted, and whether the bytes are on disk yet (M9.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldLease {
    pub lease_id: String,
    /// The contact whose object this is.
    pub owner: String,
    pub object_id: String,
    pub kind: ReplicaKind,
    /// Magnet the host fetches from when it does not have the object yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnet: Option<String>,
    /// Size the owner declared. The stored size is what actually landed.
    pub bytes: u64,
    pub received_at: u64,
    pub expires_at: u64,
    /// When the bytes were written, or `None` while the fetch is still pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_at: Option<u64>,
    /// When our signed receipt was handed back to the owner, so it is sent exactly once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_sent_at: Option<u64>,
}

impl HeldLease {
    /// Whether this lease still asks for a copy.
    pub fn is_active(&self, now: u64) -> bool {
        self.expires_at > now
    }
}

/// An object of ours that a replica could cover (M9.3).
///
/// The candidate is what the *owner* knows: the object id it published, the kind, its size, and
/// the magnet when it has one. The host resolves the pointer itself, so a candidate without a
/// magnet is still offerable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaCandidate {
    pub object_id: String,
    pub kind: ReplicaKind,
    pub magnet: Option<String>,
    pub bytes: u64,
}

/// A lease this host issued, with the metadata it sealed into it.
///
/// The owner cannot read back what it sealed (that needs the host's key), so it keeps its own
/// record: without it there would be no way to match a receipt to an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedLease {
    pub lease: ReplicaLease,
    pub object_id: String,
    pub kind: ReplicaKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnet: Option<String>,
    pub bytes: u64,
    /// Contact that was asked.
    pub contact: String,
    pub sent_at: u64,
}

// ---------------------------------------------------------------------------
// Local accounting (M9.4)
// ---------------------------------------------------------------------------

/// Free bytes on the volume holding `path`, if the platform can tell us.
///
/// A platform whose `df` does not answer this way returns `None`, and admission then trusts
/// the quota alone rather than pretending the disk is full: refusing every replica because a
/// tool is missing would be worse than the risk it guards against.
pub fn free_bytes(path: &std::path::Path) -> Option<u64> {
    let output = std::process::Command::new("df")
        .arg("-k")
        .arg("--output=avail")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let kilobytes: u64 = text
        .lines()
        .skip(1)
        .find_map(|line| line.trim().parse::<u64>().ok())?;
    Some(kilobytes.saturating_mul(1024))
}

/// Whether a host may accept one more replica of `payload`.
///
/// Every refusal here is a local rule, not a judgement about the object: hosting is off, the
/// object is larger than one replica may be, the quota would be crossed, or the free-space
/// headroom would be eaten. Refusing early is what keeps a lease from filling a disk.
pub fn admit(
    settings: &StorageSettings,
    payload: &LeasePayload,
    used_bytes: u64,
    free: Option<u64>,
) -> Result<(), ReplicaError> {
    if !settings.replicate {
        return Err(ReplicaError::NotHosting);
    }
    if payload.bytes > MAX_REPLICA_BYTES {
        return Err(ReplicaError::TooLarge {
            bytes: payload.bytes,
            cap: MAX_REPLICA_BYTES,
        });
    }
    let requested = payload.bytes.min(MAX_REPLICA_BYTES);
    if used_bytes.saturating_add(requested) > settings.quota_bytes {
        return Err(ReplicaError::Quota {
            used: used_bytes,
            requested,
            quota: settings.quota_bytes,
        });
    }
    if let Some(free) = free {
        if requested > free.saturating_sub(settings.min_free_bytes) {
            return Err(ReplicaError::LowDisk {
                free,
                headroom: settings.min_free_bytes,
            });
        }
    }
    Ok(())
}

/// What to drop, in order, to fit the quota and keep the free-space headroom.
///
/// Expired leases go first (they are already over), then the oldest live ones, so a host drops
/// the copy that is least useful rather than the one that just arrived. Space freed by the
/// plan counts towards the headroom while the plan is built, and the result is only as long as
/// the pressure requires, so a healthy host evicts nothing.
pub fn eviction_plan(
    held: &[HeldLease],
    settings: &StorageSettings,
    used_bytes: u64,
    free: Option<u64>,
    now: u64,
) -> Vec<String> {
    let mut ordered: Vec<&HeldLease> = held.iter().collect();
    ordered.sort_by_key(|lease| {
        (
            lease.is_active(now),
            lease.stored_at.unwrap_or(0),
            lease.received_at,
        )
    });

    let mut used = used_bytes;
    let mut freed = 0u64;
    let mut plan = Vec::new();
    for lease in ordered {
        let over_quota = used > settings.quota_bytes;
        let over_headroom = free
            .map(|free| free.saturating_add(freed) < settings.min_free_bytes)
            .unwrap_or(false);
        // An expired lease always goes; a live one only while there is pressure.
        if !(over_quota || over_headroom || !lease.is_active(now)) {
            break;
        }
        plan.push(lease.lease_id.clone());
        used = used.saturating_sub(lease.bytes);
        freed = freed.saturating_add(lease.bytes);
    }
    plan
}

/// Whether a host should stop accepting new replicas right now (M9.4).
///
/// Separate from [`admit`] because the caller knows the *current* usage: this is the check a
/// sync tick makes before it fetches anything for an accepted lease.
pub fn at_capacity(
    settings: &StorageSettings,
    used_bytes: u64,
    free: Option<u64>,
) -> Option<ReplicaError> {
    if !settings.replicate {
        return Some(ReplicaError::NotHosting);
    }
    if used_bytes >= settings.quota_bytes {
        return Some(ReplicaError::Quota {
            used: used_bytes,
            requested: 0,
            quota: settings.quota_bytes,
        });
    }
    if let Some(free) = free {
        if free < settings.min_free_bytes {
            return Some(ReplicaError::LowDisk {
                free,
                headroom: settings.min_free_bytes,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keypair with encryption keys, the way a profile always has them.
    fn keypair() -> KeyPair {
        let mut keypair = KeyPair::generate().unwrap();
        keypair.ensure_encryption_keys();
        keypair
    }

    /// The policy a user would save through the `storage` command.
    fn policy(quota_mib: u64, lease_days: u64) -> StoragePolicy {
        StoragePolicy {
            quota_bytes: Some(quota_mib * 1024 * 1024),
            lease_secs: Some(lease_days * 24 * 60 * 60),
            ..StoragePolicy::default()
        }
    }

    fn held(lease_id: &str, bytes: u64, stored_at: u64, expires_at: u64) -> HeldLease {
        HeldLease {
            lease_id: lease_id.into(),
            owner: "alice".into(),
            object_id: format!("object-{lease_id}"),
            kind: ReplicaKind::Profile,
            magnet: None,
            bytes,
            received_at: stored_at,
            expires_at,
            stored_at: Some(stored_at),
            receipt_sent_at: None,
        }
    }

    #[test]
    fn platform_defaults_are_resolved_against_what_the_user_and_contact_allow() {
        let desktop = StorageSettings::for_platform(Platform::Desktop);
        let mobile = StorageSettings::for_platform(Platform::Mobile);
        // A desktop volunteers space; a phone does not, and its budget is smaller.
        assert!(desktop.replicate);
        assert!(!mobile.replicate);
        assert!(mobile.quota_bytes < desktop.quota_bytes);
        assert!(mobile.lease_secs < desktop.lease_secs);
        assert_eq!(desktop.copies, 2);
        assert_eq!(mobile.copies, 1);

        // A saved override wins over the platform default.
        let mut settings = StorageSettings::for_platform(Platform::Desktop);
        settings.apply(&policy(700, 3));
        assert_eq!(settings.quota_bytes, 700 * 1024 * 1024);
        assert_eq!(settings.lease_secs, 3 * 24 * 60 * 60);
        // A per-contact rule can only narrow it.
        settings.apply_narrowing(&StoragePolicy {
            replicate: Some(false),
            quota_bytes: Some(100 * 1024 * 1024),
            lease_secs: Some(60 * 60),
            min_free_bytes: Some(2048 * 1024 * 1024),
            ..StoragePolicy::default()
        });
        assert!(!settings.replicate);
        assert_eq!(settings.quota_bytes, 100 * 1024 * 1024);
        assert_eq!(settings.lease_secs, 60 * 60);
        assert!(settings.min_free_bytes >= 2048 * 1024 * 1024);

        // A narrower rule cannot widen anything: a contact asking for more gets the local
        // settings, which is the whole point of narrowing.
        let mut settings = StorageSettings::for_platform(Platform::Mobile);
        settings.apply_narrowing(&StoragePolicy {
            quota_bytes: Some(4096 * 1024 * 1024),
            ..StoragePolicy::default()
        });
        assert_eq!(settings.quota_bytes, mobile.quota_bytes);
    }

    /// A lease is signed by its owner, addressed to one host, and its payload is sealed to it.
    #[test]
    fn a_lease_is_verified_against_its_owner_and_addressed_to_one_host() {
        let alice = keypair();
        let bob = keypair();
        let now = 1_000_000;
        let payload = LeasePayload {
            object_id: "message-1".into(),
            kind: ReplicaKind::Mailbox,
            magnet: Some("magnet:?xt=urn:btih:abc".into()),
            bytes: 4_096,
        };
        let lease = ReplicaLease::issue(
            &alice,
            &bob.fingerprint,
            bob.enc_public_key.as_deref().unwrap(),
            &payload,
            now,
            3_600,
        )
        .unwrap();
        assert!(lease
            .verify(&alice.public_key, &bob.fingerprint, now + 60)
            .is_ok());
        // Only the host it names may act on it.
        assert_eq!(
            lease
                .verify(&alice.public_key, &alice.fingerprint, now)
                .unwrap_err(),
            ReplicaError::NotHost {
                asked: bob.fingerprint.clone(),
                host: alice.fingerprint.clone()
            }
        );
        // A lease any other key signed does not speak for Alice.
        assert_eq!(
            lease
                .verify(&bob.public_key, &bob.fingerprint, now)
                .unwrap_err(),
            ReplicaError::Signature
        );
        // An expired lease is refused, and so is one from the future.
        assert!(matches!(
            lease
                .verify(&alice.public_key, &bob.fingerprint, now + 3_600)
                .unwrap_err(),
            ReplicaError::Expired { .. }
        ));
        let ahead = ReplicaLease::issue(
            &alice,
            &bob.fingerprint,
            bob.enc_public_key.as_deref().unwrap(),
            &payload,
            now + LEASE_CLOCK_SKEW_SECS + 1,
            60,
        )
        .unwrap();
        assert!(matches!(
            ahead
                .verify(&alice.public_key, &bob.fingerprint, now)
                .unwrap_err(),
            ReplicaError::NotYetValid { .. }
        ));
        // The sealed payload is what actually says what to store, so tampering with it breaks
        // the envelope.
        let mut tampered = lease.clone();
        tampered.sealed.payload_enc = "AAAAAAAA".into();
        assert_eq!(
            tampered
                .verify(&alice.public_key, &bob.fingerprint, now)
                .unwrap_err(),
            ReplicaError::Signature
        );
        // Only Bob can read what he was asked to store.
        assert_eq!(
            lease
                .open_payload(&bob, alice.enc_public_key.as_deref().unwrap())
                .unwrap(),
            payload
        );
        let carol = keypair();
        assert!(lease
            .open_payload(&carol, alice.enc_public_key.as_deref().unwrap())
            .is_err());
        assert!(lease.is_active(now + 3_599));
        assert!(!lease.is_active(now + 3_600));
        assert!(
            lease_id_for(&alice.fingerprint, &bob.fingerprint, "message-1", now)
                .starts_with("lease-")
        );
    }

    /// A receipt is the host's signed promise, and only a live one is evidence.
    #[test]
    fn a_receipt_proves_storage_and_is_refused_when_stale_or_forged() {
        let alice = keypair();
        let bob = keypair();
        let now = 1_000_000;
        let payload = LeasePayload {
            object_id: "profile-alice".into(),
            kind: ReplicaKind::Profile,
            magnet: None,
            bytes: 2_048,
        };
        let receipt = StorageReceipt::issue(
            &bob,
            "lease-1",
            &alice.fingerprint,
            now + 3_600,
            &payload,
            2_048,
            now,
        )
        .unwrap();
        assert!(receipt.verify(&bob.public_key, now + 60).is_ok());
        assert_eq!(receipt.owner, alice.fingerprint);
        assert_eq!(receipt.host, bob.fingerprint);
        // A receipt signed by someone else proves nothing.
        assert_eq!(
            receipt.verify(&alice.public_key, now).unwrap_err(),
            ReplicaError::Signature
        );
        // An expired receipt is history: the copy may be gone with the lease.
        assert!(matches!(
            receipt.verify(&bob.public_key, now + 3_600).unwrap_err(),
            ReplicaError::Expired { .. }
        ));
        assert!(receipt.is_active(now + 3_599));
        // Editing what it claims breaks the signature.
        let mut inflated = receipt.clone();
        inflated.bytes = 900_000;
        assert_eq!(
            inflated.verify(&bob.public_key, now).unwrap_err(),
            ReplicaError::Signature
        );
        let mut moved = receipt.clone();
        moved.object_id = "profile-bob".into();
        assert_eq!(
            moved.verify(&bob.public_key, now).unwrap_err(),
            ReplicaError::Signature
        );
    }

    #[test]
    fn admission_refuses_what_a_host_does_not_allow() {
        let desktop = StorageSettings::for_platform(Platform::Desktop);
        let payload = |bytes: u64| LeasePayload {
            object_id: "feed-alice".into(),
            kind: ReplicaKind::Feed,
            magnet: None,
            bytes,
        };
        let free = Some(8 * 1024 * 1024 * 1024);
        assert!(admit(&desktop, &payload(4_096), 0, free).is_ok());
        // A host that does not host refuses everything, including a tiny object.
        let off = StorageSettings::for_platform(Platform::Mobile);
        assert_eq!(
            admit(&off, &payload(4_096), 0, free).unwrap_err(),
            ReplicaError::NotHosting
        );
        // One replica may not be larger than the cap, whatever the quota says.
        let huge = StorageSettings {
            quota_bytes: u64::MAX,
            ..desktop
        };
        assert!(matches!(
            admit(&huge, &payload(MAX_REPLICA_BYTES + 1), 0, None).unwrap_err(),
            ReplicaError::TooLarge { .. }
        ));
        // Nor may it cross the quota.
        let small = StorageSettings {
            quota_bytes: 8 * 1024 * 1024,
            ..desktop
        };
        assert!(matches!(
            admit(&small, &payload(8 * 1024 * 1024), 1024 * 1024, None).unwrap_err(),
            ReplicaError::Quota { .. }
        ));
        // Nor may it eat the free-space headroom.
        let low = Some(desktop.min_free_bytes / 2);
        assert!(matches!(
            admit(&desktop, &payload(8_192), 0, low).unwrap_err(),
            ReplicaError::LowDisk { .. }
        ));
        // A platform that cannot report free space trusts the quota alone rather than refusing
        // everything.
        assert!(admit(&desktop, &payload(8_192), 0, None).is_ok());
        // Capacity is the same question asked at the current usage.
        assert!(at_capacity(&desktop, 0, free).is_none());
        assert!(matches!(
            at_capacity(&desktop, desktop.quota_bytes, free),
            Some(ReplicaError::Quota { .. })
        ));
        assert!(matches!(
            at_capacity(&desktop, 0, low),
            Some(ReplicaError::LowDisk { .. })
        ));
    }

    #[test]
    fn eviction_drops_expired_leases_first_then_the_oldest_ones() {
        let now = 1_000_000;
        let desktop = StorageSettings::for_platform(Platform::Desktop);
        let replicas = vec![
            // A live replica, stored first, which a quota squeeze should drop last.
            held("live-old", 40, now - 500, now + 10_000),
            // An expired one that has to go when anything has to go.
            held("expired", 40, now - 400, now - 1),
            // A live replica stored more recently.
            held("live-new", 40, now - 100, now + 20_000),
        ];
        // Expiry is enforced whether or not anything else is tight, and nothing live is
        // touched while there is room.
        let roomy = StorageSettings {
            quota_bytes: 1024,
            ..desktop
        };
        assert_eq!(
            eviction_plan(&replicas, &roomy, 120, None, now),
            vec!["expired".to_string()]
        );

        // Under quota pressure the oldest live copy goes next: 120 bytes of replicas against a
        // 60 byte quota has to lose two of them.
        let tight = StorageSettings {
            quota_bytes: 60,
            ..desktop
        };
        let plan = eviction_plan(&replicas, &tight, 120, None, now);
        assert_eq!(plan, vec!["expired".to_string(), "live-old".to_string()]);

        // Free-space pressure evicts even inside the quota, until the headroom is satisfied.
        let headroom = StorageSettings {
            quota_bytes: 1024,
            min_free_bytes: 100,
            ..desktop
        };
        let plan = eviction_plan(&replicas, &headroom, 120, Some(30), now);
        assert_eq!(plan.first().map(String::as_str), Some("expired"));
        assert!(
            plan.len() < 3,
            "a plan stops as soon as the pressure is gone"
        );
    }

    #[test]
    fn clamping_keeps_the_settings_usable() {
        let settings = StorageSettings {
            replicate: true,
            quota_bytes: 0,
            lease_secs: 0,
            copies: 0,
            min_free_bytes: 0,
        }
        .clamp();
        // A host that cannot store anything, or that accepts a lease for no time at all, is a
        // trap rather than a policy.
        assert!(settings.quota_bytes >= 16 * 1024 * 1024);
        assert!(settings.lease_secs >= 60 * 60);
        assert!(settings.copies >= 1);
        let settings = StorageSettings {
            quota_bytes: u64::MAX,
            lease_secs: u64::MAX,
            copies: 250,
            ..StorageSettings::default()
        }
        .clamp();
        assert!(settings.quota_bytes <= 1024 * 1024 * 1024 * 1024);
        assert!(settings.lease_secs <= 365 * 24 * 60 * 60);
        assert_eq!(settings.copies, 5);
        assert!(settings.summary().contains("hosting"));
    }
}
