//! Canonical, indexed local state for the native backend.
//!
//! The v0.3.3 applications stored a mixture of individual JSON records and a
//! `client_state.json` snapshot.  This repository imports either layout once
//! and uses SQLite as the authoritative store from then on.  Legacy files are
//! retained as a compatibility mirror until every frontend uses the daemon.

use crate::device::DeviceCertificate;
use crate::model::{ChatItem, ChatThread, Contact};
use crate::transport::sanitize_component;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use snartnet_core::{FileStorage, KeyPair, SignedPost, SignedProfile};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};

const SCHEMA_VERSION: i64 = 1;
/// Identity record holding this device's Iroh secret (ADR 0003).
///
/// It is deliberately not part of [`CanonicalState`]: that struct is also written to the
/// transitional legacy JSON mirror, and a device secret must never leave the daemon's
/// canonical identity store.
pub const DEVICE_KEY_RECORD: &str = "device_key";

/// Identity record holding this device's current [`DeviceCertificate`] (ADR 0003).
///
/// The certificate is public data (contacts must be able to read it), but replay
/// protection keys off `issued_at`, so it is kept next to the device key in the identity
/// store rather than in the mirror-visible canonical state.
pub const DEVICE_CERT_RECORD: &str = "device_certificate";

const LEGACY_KEYS: [&str; 7] = [
    "client_state",
    "keypair",
    "profile",
    "local_posts",
    "contacts",
    "threads",
    "advertise_addr",
];

/// The complete durable state owned by a native backend instance.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CanonicalState {
    pub keypair: Option<KeyPair>,
    pub profile: Option<SignedProfile>,
    #[serde(default)]
    pub posts: Vec<SignedPost>,
    #[serde(default)]
    pub contacts: Vec<Contact>,
    #[serde(default)]
    pub threads: Vec<ChatThread>,
    #[serde(default)]
    pub address: String,
    /// Relay referrals accepted from contacts, newest first (M8.3).
    ///
    /// Only the *signed* referral is stored: the grant inside it is ciphertext for this device,
    /// so the mirror file and a snapshot carry nothing an attacker could use.
    #[serde(default)]
    pub relay_referrals: Vec<crate::relay::RelayReferral>,
    /// The user's storage overrides, above the platform default and below the deployment's
    /// environment (M9.1).
    #[serde(default)]
    pub storage_policy: crate::replica::StoragePolicy,
    /// Leases this device accepted from contacts, newest first (M9.2).
    #[serde(default)]
    pub held_leases: Vec<crate::replica::HeldLease>,
    /// Leases this device asked contacts for, so a receipt can be matched to an object (M9.2).
    #[serde(default)]
    pub issued_leases: Vec<crate::replica::IssuedLease>,
    /// Receipts contacts returned for our objects, newest first (M9.2).
    #[serde(default)]
    pub receipts: Vec<crate::replica::StorageReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedObject {
    pub id: String,
    pub kind: String,
    pub owner: String,
    pub signed_created_at: String,
    pub ingestion_sequence: i64,
    pub torrent_descriptor: Option<String>,
}

/// Indexed store handle. Cheap to clone: it only holds the data directory and the
/// database path, and every operation opens its own connection.
#[derive(Clone)]
pub struct IndexedStore {
    root: PathBuf,
    db_path: PathBuf,
    /// Caps on the durable inbound spool and how much it refused (M11.1).
    ///
    /// Shared by every clone, because the peer handler writes through its own handle while the
    /// session reads the counters from the store it owns.
    spool_limits: Arc<SpoolLimits>,
}

/// The inbound spool's bounds and its refusal counter (M11.1).
struct SpoolLimits {
    entries: AtomicUsize,
    bytes: AtomicU64,
    /// Objects refused because the spool was full. Each one is an acknowledgement we did *not*
    /// write, so the sender keeps it queued and retries.
    refusals: AtomicU64,
}

impl SpoolLimits {
    fn production() -> Self {
        Self {
            entries: AtomicUsize::new(MAX_SPOOLED_INBOUND),
            bytes: AtomicU64::new(MAX_SPOOLED_BYTES),
            refusals: AtomicU64::new(0),
        }
    }
}

/// One replica indexed in the local store (M9.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaRecord {
    pub lease_id: String,
    pub owner: String,
    pub object_id: String,
    pub kind: String,
    pub bytes: u64,
    pub stored_at: i64,
    pub expires_at: i64,
}

impl ReplicaRecord {
    /// Whether this replica's lease is still inside its window.
    pub fn is_active(&self, now: i64) -> bool {
        self.expires_at > now
    }

    /// The lease as the session tracks it, so eviction and state stay one list.
    pub fn to_held_lease(&self) -> crate::replica::HeldLease {
        crate::replica::HeldLease {
            lease_id: self.lease_id.clone(),
            owner: self.owner.clone(),
            object_id: self.object_id.clone(),
            kind: match self.kind.as_str() {
                "feed" => crate::replica::ReplicaKind::Feed,
                "mailbox" => crate::replica::ReplicaKind::Mailbox,
                _ => crate::replica::ReplicaKind::Profile,
            },
            magnet: None,
            bytes: self.bytes,
            received_at: self.stored_at.max(0) as u64,
            expires_at: self.expires_at.max(0) as u64,
            stored_at: Some(self.stored_at.max(0) as u64),
            receipt_sent_at: None,
        }
    }
}

/// The signed id of one object frame, used to deduplicate delivery paths (M7.4).
///
/// Objects that carry a signed id use it; a profile is keyed by fingerprint and version;
/// anything else falls back to a hash of its bytes, so a redelivery of the same object over
/// another path still collapses onto one row.
pub fn object_id_of(object: &serde_json::Value) -> String {
    if let Some(id) = object
        .get("message")
        .and_then(|message| message.get("id"))
        .and_then(serde_json::Value::as_str)
    {
        return format!("message:{id}");
    }
    if let Some(id) = object
        .get("post")
        .and_then(|post| post.get("id"))
        .and_then(serde_json::Value::as_str)
    {
        return format!("post:{id}");
    }
    if let Some(profile) = object.get("profile") {
        if let Some(fingerprint) = profile
            .get("fingerprint")
            .and_then(serde_json::Value::as_str)
        {
            let version = profile
                .get("version")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            return format!("profile:{fingerprint}:{version}");
        }
    }
    let digest = blake3::hash(object.to_string().as_bytes())
        .to_hex()
        .to_string();
    format!("object:{digest}")
}

/// One object a peer sent, written before it was acknowledged (M7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpooledInbound {
    /// Signed object id, so a redelivery over another path is recognised.
    pub id: String,
    /// Contact that sent it.
    pub fingerprint: String,
    /// Device endpoint the frame arrived on.
    pub endpoint_id: String,
    /// The frame's object JSON, byte-for-byte.
    pub object_json: String,
    pub received_at: i64,
}

/// Most inbound objects held in the spool at once.
///
/// The spool is drained on every sync, so it only grows while syncing is paused or a hostile
/// peer floods us. Past either bound the spool *refuses* new objects instead of dropping old
/// ones: an entry in the spool is an object we already acknowledged, so discarding one would
/// turn an acknowledgement into a lie. A refusal means no acknowledgement, the sender keeps the
/// object queued, and the refusal is counted so a frontend can show the pressure (M11.1).
pub const MAX_SPOOLED_INBOUND: usize = 512;

/// Most bytes the spool holds before it refuses new objects.
///
/// A frame may be up to a megabyte (see `MAX_FRAME_BYTES`), so a count alone is not a bound: 512
/// maximum-size objects would be half a gigabyte of disk. Whichever bound is reached first stops
/// the spool, which is what keeps a flood from filling the volume the canonical store lives on.
pub const MAX_SPOOLED_BYTES: u64 = 32 * 1024 * 1024;

/// Durable sink for accepted peer objects, installed by the session (M7.3).
///
/// A peer acknowledgement means "stored", not "received into a channel", so the object has
/// to reach SQLite before the ack is written. The session drains this spool on every sync,
/// which is also what makes an object that arrived during a crash survive the restart.
pub struct InboundSpool {
    store: IndexedStore,
}

impl InboundSpool {
    pub fn new(store: IndexedStore) -> Self {
        Self { store }
    }
}

impl crate::peer::InboundPersist for InboundSpool {
    fn persist(&self, inbound: &crate::peer::PeerInbound) -> Result<(), String> {
        let crate::peer::PeerInbound::Object {
            fingerprint,
            endpoint_id,
            object,
        } = inbound
        else {
            // A notice is a hint, not an object: nothing to store before acknowledging.
            return Ok(());
        };
        self.store
            .spool_inbound(fingerprint, endpoint_id, object)
            .map(|_| ())
    }
}

impl IndexedStore {
    /// The data directory this store owns, where replicas and legacy mirror files live.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| format!("create backend root: {e}"))?;
        let store = Self {
            root: root.to_path_buf(),
            db_path: root.join("state.sqlite3"),
            spool_limits: Arc::new(SpoolLimits::production()),
        };
        let created_database = !store.db_path.exists();
        let result: Result<(), String> = (|| {
            let mut conn = store.connect()?;
            store.migrate_schema(&mut conn)?;
            if !store.is_imported(&conn)? {
                let state = store.read_legacy_state()?;
                store.backup_legacy_files()?;
                store.save_state_with_conn(&mut conn, &state)?;
                conn.execute(
                    "INSERT INTO metadata (key, value) VALUES ('legacy_import_complete', '1')",
                    [],
                )
                .map_err(|e| format!("mark legacy import complete: {e}"))?;
            }
            Ok(())
        })();
        if result.is_err() && created_database {
            for path in [
                store.db_path.clone(),
                store.db_path.with_extension("sqlite3-wal"),
                store.db_path.with_extension("sqlite3-shm"),
            ] {
                if path.exists() {
                    fs::remove_file(&path)
                        .map_err(|e| format!("roll back failed migration: {e}"))?;
                }
            }
        }
        result?;
        Ok(store)
    }

    pub fn load_state(&self) -> Result<CanonicalState, String> {
        let conn = self.connect()?;
        let keypair = read_json::<Option<KeyPair>>(&conn, "keypair")?.flatten();
        let profile = read_json::<Option<SignedProfile>>(&conn, "profile")?.flatten();
        validate_identity(keypair.as_ref(), profile.as_ref())?;
        let address = read_json::<String>(&conn, "address")?.unwrap_or_default();
        let relay_referrals =
            read_json::<Vec<crate::relay::RelayReferral>>(&conn, "relay_referrals")?
                .unwrap_or_default();
        let storage_policy = read_json::<crate::replica::StoragePolicy>(&conn, "storage_policy")?
            .unwrap_or_default();
        let held_leases =
            read_json::<Vec<crate::replica::HeldLease>>(&conn, "held_leases")?.unwrap_or_default();
        let issued_leases = read_json::<Vec<crate::replica::IssuedLease>>(&conn, "issued_leases")?
            .unwrap_or_default();
        let receipts = read_json::<Vec<crate::replica::StorageReceipt>>(&conn, "receipts")?
            .unwrap_or_default();

        let posts = read_rows::<SignedPost>(&conn, "post")?;
        let contacts = read_table::<Contact>(&conn, "contacts", "fingerprint")?;
        let threads = read_table::<ChatThread>(&conn, "threads", "fingerprint")?;
        Ok(CanonicalState {
            keypair,
            profile,
            posts,
            contacts,
            threads,
            address,
            relay_referrals,
            storage_policy,
            held_leases,
            issued_leases,
            receipts,
        })
    }

    /// Commit a complete state snapshot. Immutable objects are append-only and
    /// de-duplicated by signed object ID; mutable contact and thread summaries
    /// are replaced transactionally.
    pub fn save_state(&self, state: &CanonicalState) -> Result<(), String> {
        validate_identity(state.keypair.as_ref(), state.profile.as_ref())?;
        let mut conn = self.connect()?;
        self.save_state_with_conn(&mut conn, state)?;
        // The SQLite transaction is authoritative. A legacy writer may be
        // unavailable during the M1 transition, but that must not turn a
        // committed canonical update into an apparent failure.
        let _ = self.write_legacy_mirror(state);
        Ok(())
    }

    /// This device's persisted Iroh secret, if one has been generated yet.
    pub fn device_key(&self) -> Result<Option<String>, String> {
        let conn = self.connect()?;
        read_json::<String>(&conn, DEVICE_KEY_RECORD)
    }

    /// Persist this device's Iroh secret as a canonical identity record.
    pub fn save_device_key(&self, secret: &str) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO identity_records (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![DEVICE_KEY_RECORD, to_json(&secret)?],
        )
        .map_err(|e| format!("save device key: {e}"))?;
        Ok(())
    }

    /// The certificate this device last issued for itself, if any.
    ///
    /// Reusing it across restarts keeps `issued_at` (and therefore every contact's pin)
    /// stable until the certificate actually expires.
    pub fn device_certificate(&self) -> Result<Option<DeviceCertificate>, String> {
        let conn = self.connect()?;
        read_json::<DeviceCertificate>(&conn, DEVICE_CERT_RECORD)
    }

    /// Persist this device's certificate as a canonical identity record.
    pub fn save_device_certificate(&self, certificate: &DeviceCertificate) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO identity_records (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![DEVICE_CERT_RECORD, to_json(certificate)?],
        )
        .map_err(|e| format!("save device certificate: {e}"))?;
        Ok(())
    }

    pub fn objects_after(&self, sequence: i64) -> Result<Vec<IndexedObject>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, owner, signed_created_at, ingestion_sequence, torrent_descriptor
                 FROM objects WHERE ingestion_sequence > ?1 ORDER BY ingestion_sequence",
            )
            .map_err(|e| format!("prepare indexed object query: {e}"))?;
        let rows = stmt
            .query_map(params![sequence], |row| {
                Ok(IndexedObject {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    owner: row.get(2)?,
                    signed_created_at: row.get(3)?,
                    ingestion_sequence: row.get(4)?,
                    torrent_descriptor: row.get(5)?,
                })
            })
            .map_err(|e| format!("query indexed objects: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read indexed objects: {e}"))
    }

    pub fn set_cursor(&self, name: &str, sequence: i64) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO cursors (name, sequence) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET sequence = excluded.sequence",
            params![name, sequence],
        )
        .map_err(|e| format!("save cursor: {e}"))?;
        Ok(())
    }

    pub fn cursor(&self, name: &str) -> Result<i64, String> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT sequence FROM cursors WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or(0))
        .map_err(|e| format!("load cursor: {e}"))
    }

    pub fn set_torrent_descriptor(&self, object_id: &str, descriptor: &str) -> Result<(), String> {
        let conn = self.connect()?;
        let changed = conn
            .execute(
                "UPDATE objects SET torrent_descriptor = ?2 WHERE id = ?1",
                params![object_id, descriptor],
            )
            .map_err(|e| format!("save torrent descriptor: {e}"))?;
        if changed == 0 {
            return Err(format!(
                "cannot set descriptor for unknown object {object_id}"
            ));
        }
        Ok(())
    }

    /// Narrow (or widen) the spool bounds (M11.1).
    ///
    /// The session applies its [`crate::limits::Limits`] here, so a test can reach a full spool
    /// with a handful of objects instead of half a gigabyte of them.
    pub fn set_spool_limits(&self, entries: usize, bytes: u64) {
        self.spool_limits.entries.store(entries, Ordering::Relaxed);
        self.spool_limits.bytes.store(bytes, Ordering::Relaxed);
    }

    /// Objects this store refused because the spool was full (M11.1).
    ///
    /// Each refusal is an acknowledgement that was *not* written, so a growing counter means
    /// inbound is being turned away and the daemon should slow down rather than retry sooner.
    pub fn spool_refusals(&self) -> u64 {
        self.spool_limits.refusals.load(Ordering::Relaxed)
    }

    /// Bytes the spool currently holds.
    pub fn spool_bytes(&self) -> Result<u64, String> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(object_json)), 0) FROM inbound_spool",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|bytes| bytes.max(0) as u64)
        .map_err(|e| format!("measure inbound spool: {e}"))
    }

    /// Write a peer's object to the durable spool before it is acknowledged (M7.3).
    ///
    /// The object id comes from the signed payload when it has one, so a redelivery of the
    /// same object over the torrent or iroh path collapses onto one row. A full spool refuses
    /// the write with an error, which propagates as a refused acknowledgement (M11.1): the
    /// object is neither stored nor claimed, and the sender retries later.
    pub fn spool_inbound(
        &self,
        fingerprint: &str,
        endpoint_id: &str,
        object: &serde_json::Value,
    ) -> Result<String, String> {
        let id = object_id_of(object);
        let json = object.to_string();
        let conn = self.connect()?;
        // A redelivery of an object the spool already holds only replaces its row, so it is
        // always allowed: refusing it would claim a bound was reached when nothing grew.
        let known: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM inbound_spool WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| format!("look up spooled object: {e}"))?;
        if !known {
            let (entries, bytes): (i64, i64) = conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(LENGTH(object_json)), 0) FROM inbound_spool",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| format!("measure inbound spool: {e}"))?;
            // Checked before the insert, never by trimming afterwards: trimming would drop an
            // object that a peer was already told we stored (M11.1).
            let over_entries =
                entries as usize >= self.spool_limits.entries.load(Ordering::Relaxed);
            let over_bytes = bytes.max(0) as u64 + json.len() as u64
                > self.spool_limits.bytes.load(Ordering::Relaxed);
            if over_entries || over_bytes {
                self.spool_limits.refusals.fetch_add(1, Ordering::Relaxed);
                return Err(format!(
                    "inbound spool is full ({} object(s), {} byte(s))",
                    entries, bytes
                ));
            }
        }
        conn.execute(
            "INSERT INTO inbound_spool (id, fingerprint, endpoint_id, object_json, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 object_json = excluded.object_json,
                 endpoint_id = excluded.endpoint_id,
                 received_at = excluded.received_at",
            params![id, fingerprint, endpoint_id, json, Utc::now().timestamp()],
        )
        .map_err(|e| format!("spool inbound object: {e}"))?;
        Ok(id)
    }

    /// Spooled objects in arrival order, oldest first.
    pub fn spooled_inbound(&self) -> Result<Vec<SpooledInbound>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, fingerprint, endpoint_id, object_json, received_at
                 FROM inbound_spool ORDER BY received_at, rowid",
            )
            .map_err(|e| format!("prepare inbound spool query: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SpooledInbound {
                    id: row.get(0)?,
                    fingerprint: row.get(1)?,
                    endpoint_id: row.get(2)?,
                    object_json: row.get(3)?,
                    received_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("query inbound spool: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read inbound spool: {e}"))
    }

    /// Drop spooled objects the session has ingested.
    pub fn clear_spooled(&self, ids: &[String]) -> Result<(), String> {
        let conn = self.connect()?;
        for id in ids {
            conn.execute("DELETE FROM inbound_spool WHERE id = ?1", params![id])
                .map_err(|e| format!("clear spooled object: {e}"))?;
        }
        Ok(())
    }

    /// How many objects are waiting to be ingested, for diagnostics.
    pub fn spooled_count(&self) -> Result<usize, String> {
        let conn = self.connect()?;
        conn.query_row("SELECT COUNT(*) FROM inbound_spool", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count.max(0) as usize)
        .map_err(|e| format!("count spooled objects: {e}"))
    }

    /// Where one replica's bytes live.
    fn replica_path(&self, lease_id: &str) -> PathBuf {
        self.root
            .join("replicas")
            .join(format!("{}.bin", sanitize_component(lease_id)))
    }

    /// Write one replica to disk and index it (M9.3).
    ///
    /// The bytes go to a file rather than into SQLite: a replica can be megabytes and is
    /// written once and read rarely, which is what a file is for. The row is what makes it
    /// evictable, since expiry and usage have to be queryable without touching the files.
    pub fn save_replica(
        &self,
        lease_id: &str,
        owner: &str,
        object_id: &str,
        kind: &str,
        bytes: &[u8],
        expires_at: u64,
    ) -> Result<(), String> {
        let dir = self.root.join("replicas");
        fs::create_dir_all(&dir).map_err(|e| format!("create replica area: {e}"))?;
        fs::write(self.replica_path(lease_id), bytes)
            .map_err(|e| format!("write replica {lease_id}: {e}"))?;
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO replicas (lease_id, owner, object_id, kind, bytes, stored_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(lease_id) DO UPDATE SET
                 owner = excluded.owner,
                 object_id = excluded.object_id,
                 kind = excluded.kind,
                 bytes = excluded.bytes,
                 stored_at = excluded.stored_at,
                 expires_at = excluded.expires_at",
            params![
                lease_id,
                owner,
                object_id,
                kind,
                bytes.len() as i64,
                Utc::now().timestamp(),
                expires_at as i64
            ],
        )
        .map_err(|e| format!("index replica {lease_id}: {e}"))?;
        Ok(())
    }

    /// One replica's bytes, when this host still holds them.
    pub fn load_replica(&self, lease_id: &str) -> Option<Vec<u8>> {
        fs::read(self.replica_path(lease_id)).ok()
    }

    /// Every indexed replica, oldest first so eviction can walk it in order.
    pub fn replica_records(&self) -> Result<Vec<ReplicaRecord>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT lease_id, owner, object_id, kind, bytes, stored_at, expires_at
                 FROM replicas ORDER BY stored_at, lease_id",
            )
            .map_err(|e| format!("prepare replica query: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ReplicaRecord {
                    lease_id: row.get(0)?,
                    owner: row.get(1)?,
                    object_id: row.get(2)?,
                    kind: row.get(3)?,
                    bytes: row.get::<_, i64>(4)?.max(0) as u64,
                    stored_at: row.get(5)?,
                    expires_at: row.get(6)?,
                })
            })
            .map_err(|e| format!("query replicas: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read replicas: {e}"))
    }

    /// Bytes this host currently dedicates to replicas.
    pub fn replica_usage(&self) -> Result<u64, String> {
        let conn = self.connect()?;
        conn.query_row("SELECT COALESCE(SUM(bytes), 0) FROM replicas", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|bytes| bytes.max(0) as u64)
        .map_err(|e| format!("sum replica bytes: {e}"))
    }

    /// Drop replicas by lease id, returning how many were removed (M9.4).
    ///
    /// The file is removed first: an index row without its bytes is a lie, while a file
    /// without a row is only wasted space that the next write of that lease reuses.
    pub fn remove_replicas(&self, lease_ids: &[String]) -> Result<usize, String> {
        let conn = self.connect()?;
        let mut removed = 0;
        for lease_id in lease_ids {
            let _ = fs::remove_file(self.replica_path(lease_id));
            removed += conn
                .execute(
                    "DELETE FROM replicas WHERE lease_id = ?1",
                    params![lease_id],
                )
                .map_err(|e| format!("drop replica {lease_id}: {e}"))?;
        }
        Ok(removed)
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn =
            Connection::open(&self.db_path).map_err(|e| format!("open indexed state: {e}"))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )
        .map_err(|e| format!("configure indexed state: {e}"))?;
        Ok(conn)
    }

    fn migrate_schema(&self, conn: &mut Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS identity_records (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS objects (
                 id TEXT PRIMARY KEY,
                 kind TEXT NOT NULL,
                 owner TEXT NOT NULL,
                 signed_created_at TEXT NOT NULL,
                 ingestion_sequence INTEGER NOT NULL UNIQUE,
                 torrent_descriptor TEXT,
                 payload_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS objects_owner_time ON objects(owner, signed_created_at, id);
             CREATE TABLE IF NOT EXISTS contacts (fingerprint TEXT PRIMARY KEY, payload_json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS threads (fingerprint TEXT PRIMARY KEY, payload_json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS cursors (name TEXT PRIMARY KEY, sequence INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS inbound_spool (
                 id TEXT PRIMARY KEY,
                 fingerprint TEXT NOT NULL,
                 endpoint_id TEXT NOT NULL,
                 object_json TEXT NOT NULL,
                 received_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS inbound_spool_arrival ON inbound_spool(received_at, id);
             CREATE TABLE IF NOT EXISTS replicas (
                 lease_id TEXT PRIMARY KEY,
                 owner TEXT NOT NULL,
                 object_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 bytes INTEGER NOT NULL,
                 stored_at INTEGER NOT NULL,
                 expires_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS replicas_owner_object ON replicas(owner, object_id);
             CREATE INDEX IF NOT EXISTS replicas_expiry ON replicas(expires_at, stored_at);",
        )
        .map_err(|e| format!("create indexed-state schema: {e}"))?;
        let version = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("read schema version: {e}"))?;
        match version {
            Some(value) if value.parse::<i64>().ok() == Some(SCHEMA_VERSION) => Ok(()),
            Some(value) => Err(format!("unsupported indexed-state schema version {value}")),
            None => {
                conn.execute(
                    "INSERT INTO metadata (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION],
                )
                .map_err(|e| format!("write schema version: {e}"))?;
                Ok(())
            }
        }
    }

    fn is_imported(&self, conn: &Connection) -> Result<bool, String> {
        conn.query_row(
            "SELECT value FROM metadata WHERE key = 'legacy_import_complete'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map(|value| value.as_deref() == Some("1"))
        .map_err(|e| format!("read legacy import status: {e}"))
    }

    fn save_state_with_conn(
        &self,
        conn: &mut Connection,
        state: &CanonicalState,
    ) -> Result<(), String> {
        validate_identity(state.keypair.as_ref(), state.profile.as_ref())?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("start indexed-state transaction: {e}"))?;
        write_json(&tx, "keypair", &state.keypair)?;
        write_json(&tx, "profile", &state.profile)?;
        write_json(&tx, "address", &state.address)?;
        write_json(&tx, "relay_referrals", &state.relay_referrals)?;
        write_json(&tx, "storage_policy", &state.storage_policy)?;
        write_json(&tx, "held_leases", &state.held_leases)?;
        write_json(&tx, "issued_leases", &state.issued_leases)?;
        write_json(&tx, "receipts", &state.receipts)?;
        tx.execute("DELETE FROM contacts", [])
            .map_err(|e| format!("replace contacts: {e}"))?;
        for contact in &state.contacts {
            tx.execute(
                "INSERT INTO contacts (fingerprint, payload_json) VALUES (?1, ?2)",
                params![contact.fingerprint, to_json(contact)?],
            )
            .map_err(|e| format!("save contact: {e}"))?;
        }
        tx.execute("DELETE FROM threads", [])
            .map_err(|e| format!("replace threads: {e}"))?;
        for thread in &state.threads {
            tx.execute(
                "INSERT INTO threads (fingerprint, payload_json) VALUES (?1, ?2)",
                params![thread.contact_fingerprint, to_json(thread)?],
            )
            .map_err(|e| format!("save thread: {e}"))?;
        }
        if let Some(profile) = &state.profile {
            insert_object(
                &tx,
                &format!(
                    "profile:{}:{}",
                    profile.profile.fingerprint, profile.profile.version
                ),
                "profile",
                &profile.profile.fingerprint,
                profile.profile.updated_at,
                &to_json(profile)?,
            )?;
        }
        for post in &state.posts {
            insert_object(
                &tx,
                &post.post.id,
                "post",
                &post.post.author_fingerprint,
                post.post.created_at,
                &to_json(post)?,
            )?;
        }
        for thread in &state.threads {
            for item in &thread.messages {
                insert_message_object(&tx, &thread.contact_fingerprint, item)?;
            }
        }
        tx.commit()
            .map_err(|e| format!("commit indexed-state transaction: {e}"))
    }

    fn read_legacy_state(&self) -> Result<CanonicalState, String> {
        let storage = FileStorage::new(self.root.join("data")).map_err(|e| e.to_string())?;
        let snapshot = storage
            .get_json::<CanonicalState>("client_state")
            .map_err(|e| format!("read legacy client_state: {e}"))?;
        let split = CanonicalState {
            keypair: storage.get_json("keypair").map_err(|e| e.to_string())?,
            profile: storage.get_json("profile").map_err(|e| e.to_string())?,
            posts: storage
                .get_json("local_posts")
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
            contacts: storage
                .get_json("contacts")
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
            threads: storage
                .get_json("threads")
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
            address: storage
                .get_json("advertise_addr")
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
            // The legacy record predates relay referrals (M8.3) and storage policy (M9), so an
            // imported state starts with none and learns them from contacts and the platform.
            relay_referrals: Vec::new(),
            storage_policy: crate::replica::StoragePolicy::default(),
            held_leases: Vec::new(),
            issued_leases: Vec::new(),
            receipts: Vec::new(),
        };
        merge_legacy(snapshot, split)
    }

    fn backup_legacy_files(&self) -> Result<(), String> {
        let data = self.root.join("data");
        if !data.exists() {
            return Ok(());
        }
        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let backup = self.root.join("backups").join(format!("legacy-{stamp}"));
        let mut copied = false;
        for key in LEGACY_KEYS {
            let source = data.join(format!("{key}.json"));
            if source.exists() {
                fs::create_dir_all(&backup).map_err(|e| format!("create legacy backup: {e}"))?;
                fs::copy(&source, backup.join(format!("{key}.json")))
                    .map_err(|e| format!("back up legacy {key}: {e}"))?;
                copied = true;
            }
        }
        if !copied && backup.exists() {
            fs::remove_dir(&backup).map_err(|e| format!("remove empty legacy backup: {e}"))?;
        }
        Ok(())
    }

    fn write_legacy_mirror(&self, state: &CanonicalState) -> Result<(), String> {
        let storage = FileStorage::new(self.root.join("data")).map_err(|e| e.to_string())?;
        storage
            .set_json("client_state", state)
            .map_err(|e| e.to_string())?;
        storage
            .set_json("keypair", &state.keypair)
            .map_err(|e| e.to_string())?;
        storage
            .set_json("profile", &state.profile)
            .map_err(|e| e.to_string())?;
        storage
            .set_json("local_posts", &state.posts)
            .map_err(|e| e.to_string())?;
        storage
            .set_json("contacts", &state.contacts)
            .map_err(|e| e.to_string())?;
        storage
            .set_json("threads", &state.threads)
            .map_err(|e| e.to_string())?;
        storage
            .set_json("advertise_addr", &state.address)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn merge_legacy(
    snapshot: Option<CanonicalState>,
    split: CanonicalState,
) -> Result<CanonicalState, String> {
    let mut state = snapshot.unwrap_or_default();
    validate_identity(state.keypair.as_ref(), state.profile.as_ref())?;
    validate_identity(split.keypair.as_ref(), split.profile.as_ref())?;
    if let (Some(a), Some(b)) = (&state.profile, &split.profile) {
        if a.profile.fingerprint != b.profile.fingerprint {
            return Err("legacy storage contains conflicting profile identities".into());
        }
        if b.profile.updated_at > a.profile.updated_at {
            state.profile = Some(b.clone());
        }
    }
    if state.keypair.is_none() {
        state.keypair = split.keypair;
    }
    if state.profile.is_none() {
        state.profile = split.profile;
    }
    if state.address.is_empty() {
        state.address = split.address;
    }
    union_by_id(&mut state.posts, split.posts, |post| post.post.id.clone());
    union_by_id(&mut state.contacts, split.contacts, |contact| {
        contact.fingerprint.clone()
    });
    merge_threads(&mut state.threads, split.threads);
    validate_identity(state.keypair.as_ref(), state.profile.as_ref())?;
    Ok(state)
}

fn union_by_id<T, F>(target: &mut Vec<T>, incoming: Vec<T>, id: F)
where
    F: Fn(&T) -> String,
{
    for value in incoming {
        if !target.iter().any(|existing| id(existing) == id(&value)) {
            target.push(value);
        }
    }
}

fn merge_threads(target: &mut Vec<ChatThread>, incoming: Vec<ChatThread>) {
    for mut thread in incoming {
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.contact_fingerprint == thread.contact_fingerprint)
        {
            for item in thread.messages.drain(..) {
                if !existing
                    .messages
                    .iter()
                    .any(|message| message.id == item.id)
                {
                    existing.messages.push(item);
                }
            }
            existing.unread_count = existing.unread_count.max(thread.unread_count);
        } else {
            target.push(thread);
        }
    }
}

fn validate_identity(
    keypair: Option<&KeyPair>,
    profile: Option<&SignedProfile>,
) -> Result<(), String> {
    if let Some(profile) = profile {
        let keypair = keypair.ok_or("stored profile is missing its keypair")?;
        if keypair.fingerprint != profile.profile.fingerprint || !profile.verify().unwrap_or(false)
        {
            return Err("stored identity is invalid; restore a backup before continuing".into());
        }
    }
    Ok(())
}

fn to_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("serialize indexed state: {e}"))
}

fn write_json<T: Serialize>(tx: &Transaction<'_>, key: &str, value: &T) -> Result<(), String> {
    tx.execute(
        "INSERT INTO identity_records (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![key, to_json(value)?],
    )
    .map_err(|e| format!("save identity record: {e}"))?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    conn: &Connection,
    key: &str,
) -> Result<Option<T>, String> {
    let value = conn
        .query_row(
            "SELECT value_json FROM identity_records WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("read identity record: {e}"))?;
    value
        .map(|json| serde_json::from_str(&json).map_err(|e| format!("decode identity record: {e}")))
        .transpose()
}

fn read_rows<T: for<'de> Deserialize<'de>>(
    conn: &Connection,
    kind: &str,
) -> Result<Vec<T>, String> {
    let mut stmt = conn
        .prepare("SELECT payload_json FROM objects WHERE kind = ?1 ORDER BY signed_created_at, id")
        .map_err(|e| format!("prepare object load: {e}"))?;
    let rows = stmt
        .query_map(params![kind], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query objects: {e}"))?;
    rows.map(|row| {
        let json = row.map_err(|e| format!("read object: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("decode object: {e}"))
    })
    .collect()
}

fn read_table<T: for<'de> Deserialize<'de>>(
    conn: &Connection,
    table: &str,
    order_column: &str,
) -> Result<Vec<T>, String> {
    let sql = format!("SELECT payload_json FROM {table} ORDER BY {order_column}");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare {table} load: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query {table}: {e}"))?;
    rows.map(|row| {
        let json = row.map_err(|e| format!("read {table}: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("decode {table}: {e}"))
    })
    .collect()
}

fn insert_object(
    tx: &Transaction<'_>,
    id: &str,
    kind: &str,
    owner: &str,
    created_at: DateTime<Utc>,
    payload_json: &str,
) -> Result<(), String> {
    let exists = tx
        .query_row("SELECT 1 FROM objects WHERE id = ?1", params![id], |_| {
            Ok(())
        })
        .optional()
        .map_err(|e| format!("look up indexed object: {e}"))?
        .is_some();
    if exists {
        return Ok(());
    }
    let next = tx
        .query_row(
            "SELECT COALESCE(MAX(ingestion_sequence), 0) + 1 FROM objects",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| format!("allocate ingestion sequence: {e}"))?;
    tx.execute(
        "INSERT INTO objects (id, kind, owner, signed_created_at, ingestion_sequence, torrent_descriptor, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
        params![id, kind, owner, created_at.to_rfc3339(), next, payload_json],
    )
    .map_err(|e| format!("insert indexed object: {e}"))?;
    Ok(())
}

fn insert_message_object(
    tx: &Transaction<'_>,
    contact: &str,
    item: &ChatItem,
) -> Result<(), String> {
    let created_at = item
        .envelope
        .as_ref()
        .map(|envelope| envelope.message.created_at)
        .unwrap_or_else(Utc::now);
    insert_object(
        tx,
        &item.id,
        "message",
        contact,
        created_at,
        &to_json(item)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKey, DEFAULT_CAPABILITIES};
    use chrono::Duration;
    use snartnet_core::{Post, Profile, SignedPost};

    fn state(username: &str) -> CanonicalState {
        let keypair = KeyPair::generate().unwrap();
        let profile = SignedProfile::create(
            Profile::new(username.into(), keypair.get_public_info()),
            &keypair,
        )
        .unwrap();
        CanonicalState {
            keypair: Some(keypair),
            profile: Some(profile),
            ..Default::default()
        }
    }

    #[test]
    fn saves_deduplicated_objects_and_resumable_cursors() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        let mut state = state("alice");
        let keypair = state.keypair.clone().unwrap();
        let fingerprint = state.profile.as_ref().unwrap().profile.fingerprint.clone();
        let post = SignedPost::create(Post::new(fingerprint, "hello".into(), None, None), &keypair)
            .unwrap();
        state.posts.push(post.clone());
        state.posts.push(post);
        store.save_state(&state).unwrap();
        let objects = store.objects_after(0).unwrap();
        assert_eq!(
            objects
                .iter()
                .filter(|object| object.kind == "post")
                .count(),
            1
        );
        let last = objects.last().unwrap().ingestion_sequence;
        store.set_cursor("peer:alice", last).unwrap();
        assert_eq!(store.cursor("peer:alice").unwrap(), last);
        assert!(store.objects_after(last).unwrap().is_empty());
        let post_id = state.posts[0].post.id.clone();
        store
            .set_torrent_descriptor(&post_id, "magnet:?xt=urn:btih:test")
            .unwrap();
        assert_eq!(
            store
                .objects_after(0)
                .unwrap()
                .into_iter()
                .find(|object| object.id == post_id)
                .unwrap()
                .torrent_descriptor
                .as_deref(),
            Some("magnet:?xt=urn:btih:test")
        );
    }

    #[test]
    fn imports_split_legacy_records_and_backs_them_up() {
        let root = tempfile::tempdir().unwrap();
        let legacy = FileStorage::new(root.path().join("data")).unwrap();
        let state = state("alice");
        legacy.set_json("keypair", &state.keypair).unwrap();
        legacy.set_json("profile", &state.profile).unwrap();
        legacy.set_json("local_posts", &state.posts).unwrap();
        legacy.set_json("contacts", &state.contacts).unwrap();
        legacy.set_json("threads", &state.threads).unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        assert_eq!(
            store
                .load_state()
                .unwrap()
                .profile
                .unwrap()
                .profile
                .username,
            "alice"
        );
        assert!(root
            .path()
            .join("backups")
            .read_dir()
            .unwrap()
            .next()
            .is_some());
    }

    #[test]
    fn rejects_conflicting_legacy_identities_without_creating_database() {
        let root = tempfile::tempdir().unwrap();
        let legacy = FileStorage::new(root.path().join("data")).unwrap();
        let first = state("alice");
        let second = state("bob");
        legacy.set_json("client_state", &first).unwrap();
        legacy.set_json("keypair", &second.keypair).unwrap();
        legacy.set_json("profile", &second.profile).unwrap();
        assert!(IndexedStore::open(root.path()).is_err());
        assert!(!root.path().join("backups").exists());
        assert!(!root.path().join("state.sqlite3").exists());
    }

    #[test]
    fn invalid_legacy_profile_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let legacy = FileStorage::new(root.path().join("data")).unwrap();
        legacy.set_item("client_state", "not-json").unwrap();
        assert!(IndexedStore::open(root.path()).is_err());
    }

    #[test]
    fn detects_corrupt_canonical_records() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        store.save_state(&state("alice")).unwrap();
        let conn = Connection::open(root.path().join("state.sqlite3")).unwrap();
        conn.execute(
            "UPDATE identity_records SET value_json = '{' WHERE key = 'profile'",
            [],
        )
        .unwrap();
        assert!(store.load_state().is_err());
    }

    #[test]
    fn ingestion_cursors_remain_monotonic_when_signed_clocks_skew() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        let mut state = state("alice");
        let keypair = state.keypair.clone().unwrap();
        let fingerprint = state.profile.as_ref().unwrap().profile.fingerprint.clone();
        let first = SignedPost::create(
            Post::new(fingerprint.clone(), "current clock".into(), None, None),
            &keypair,
        )
        .unwrap();
        state.posts.push(first);
        store.save_state(&state).unwrap();
        let cursor = store
            .objects_after(0)
            .unwrap()
            .last()
            .unwrap()
            .ingestion_sequence;

        let mut old_post = Post::new(fingerprint, "skewed clock".into(), None, None);
        old_post.created_at -= Duration::days(30);
        state
            .posts
            .push(SignedPost::create(old_post, &keypair).unwrap());
        store.save_state(&state).unwrap();

        let new_objects = store.objects_after(cursor).unwrap();
        assert_eq!(new_objects.len(), 1);
        assert!(new_objects[0].ingestion_sequence > cursor);
    }

    #[test]
    fn the_device_key_round_trips_through_the_identity_table() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        assert_eq!(store.device_key().unwrap(), None);
        store.save_device_key("device-secret").unwrap();
        assert_eq!(
            store.device_key().unwrap().as_deref(),
            Some("device-secret")
        );
        // Reopening the store must return the same identity, or every restart would
        // present a new endpoint id to contacts.
        drop(store);
        let reopened = IndexedStore::open(root.path()).unwrap();
        assert_eq!(
            reopened.device_key().unwrap().as_deref(),
            Some("device-secret")
        );
    }

    #[test]
    fn the_device_certificate_round_trips_and_survives_restarts() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        assert_eq!(store.device_certificate().unwrap(), None);
        let state = state("alice");
        let profile = state.profile.clone().unwrap();
        let keypair = state.keypair.clone().unwrap();
        let device = DeviceKey::generate();
        let certificate = DeviceCertificate::issue(
            &profile,
            &keypair,
            &device,
            &DEFAULT_CAPABILITIES,
            1_000,
            600,
        )
        .unwrap();
        store.save_device_certificate(&certificate).unwrap();
        assert_eq!(
            store.device_certificate().unwrap(),
            Some(certificate.clone())
        );
        // A restart must hand out the same `issued_at`, or contacts would see a replay-looking
        // bump on every launch.
        drop(store);
        let reopened = IndexedStore::open(root.path()).unwrap();
        assert_eq!(reopened.device_certificate().unwrap(), Some(certificate));
    }

    /// A full spool must refuse, not trim: an entry it dropped was an object we already
    /// acknowledged, and the sender has no way to learn that (M11.1).
    #[test]
    fn a_full_spool_refuses_new_objects_instead_of_dropping_acknowledged_ones() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        store.set_spool_limits(2, MAX_SPOOLED_BYTES);
        let object = |id: &str| serde_json::json!({ "message": { "id": id } });
        store
            .spool_inbound("alice", "endpoint", &object("one"))
            .unwrap();
        store
            .spool_inbound("alice", "endpoint", &object("two"))
            .unwrap();
        // A redelivery of something already spooled only replaces its row, so it is allowed.
        store
            .spool_inbound("alice", "endpoint", &object("one"))
            .unwrap();
        assert_eq!(store.spool_refusals(), 0);
        let refused = store.spool_inbound("alice", "endpoint", &object("three"));
        assert!(refused.is_err());
        assert_eq!(store.spool_refusals(), 1);
        // Both earlier objects are still there: nothing an acknowledged peer was dropped.
        assert_eq!(store.spooled_count().unwrap(), 2);
        let ids: Vec<String> = store
            .spooled_inbound()
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert!(ids.contains(&"message:one".to_string()));
        assert!(ids.contains(&"message:two".to_string()));
        // Draining a slot makes room again, so a refused object can arrive later.
        store.clear_spooled(&["message:two".to_string()]).unwrap();
        store
            .spool_inbound("alice", "endpoint", &object("three"))
            .unwrap();
        assert_eq!(store.spooled_count().unwrap(), 2);
    }

    /// A count alone is not a bound for megabyte frames, so the byte cap stops the spool too.
    #[test]
    fn the_spool_stops_at_its_byte_bound() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        store.set_spool_limits(MAX_SPOOLED_INBOUND, 200);
        let object =
            |id: &str| serde_json::json!({ "message": { "id": id, "content": "x".repeat(80) } });
        store
            .spool_inbound("alice", "endpoint", &object("one"))
            .unwrap();
        let bytes = store.spool_bytes().unwrap();
        assert!(bytes > 0);
        let err = store
            .spool_inbound("alice", "endpoint", &object("two"))
            .unwrap_err();
        assert!(err.contains("spool is full"), "{err}");
        assert_eq!(store.spool_refusals(), 1);
    }

    #[test]
    fn the_device_key_never_reaches_the_legacy_mirror_or_the_snapshot_state() {
        let root = tempfile::tempdir().unwrap();
        let store = IndexedStore::open(root.path()).unwrap();
        let state = state("alice");
        store.save_state(&state).unwrap();
        store.save_device_key("device-secret").unwrap();
        let mirror = fs::read_to_string(root.path().join("data/client_state.json")).unwrap();
        assert!(!mirror.contains("device-secret"));
        assert!(!mirror.contains(DEVICE_KEY_RECORD));
        // CanonicalState is what snapshots and the legacy mirror serialize, so the secret
        // must not be reachable from it at all.
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(!serialized.contains("device_key"));
    }
}
