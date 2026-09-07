//! Versioned wire objects shared by the direct and torrent transports.
//!
//! Torrent hashes prove that a peer returned the bytes belonging to a swarm;
//! these objects prove who authored those bytes and who may read them.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use blake3::Hasher;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use snartnet_core::{KeyPair, SignedMessage, SignedPost};
use std::collections::BTreeMap;

pub const WIRE_VERSION: u16 = 1;
pub const ENCRYPTION_ALGORITHM: &str = "chacha20poly1305-x25519-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Profile,
    Post,
    FeedManifest,
    DirectMessage,
    MailboxManifest,
    Control,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub v: u16,
    pub kind: ObjectType,
    pub id: String,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_enc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    pub body: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    #[serde(default)]
    pub meta: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    pub envelope: Envelope,
    pub signature: String,
}

impl Envelope {
    pub fn canonical_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("envelope serialization failed: {e}"))
    }

    pub fn validate(&self, recipient: Option<&str>) -> Result<(), String> {
        if self.v != WIRE_VERSION {
            return Err(format!("unsupported wire version {}", self.v));
        }
        if self.id.trim().is_empty() || self.from.trim().is_empty() {
            return Err("envelope identity fields are empty".into());
        }
        if let Some(recipient) = recipient {
            if !self.to.iter().any(|value| value == recipient) {
                return Err("envelope is addressed to another recipient".into());
            }
        }
        if let Some(expiry) = self.expires_at {
            if expiry <= self.created_at {
                return Err("envelope expiry precedes creation".into());
            }
        }
        if self.kind == ObjectType::DirectMessage {
            if self.to.len() != 1 {
                return Err("direct messages must have exactly one recipient".into());
            }
            if self.body_enc.as_deref() != Some(ENCRYPTION_ALGORITHM) {
                return Err("direct messages must use the supported encryption algorithm".into());
            }
            if self.nonce.as_deref().is_none() {
                return Err("encrypted messages require a nonce".into());
            }
        }
        Ok(())
    }
}

impl SignedEnvelope {
    pub fn sign(envelope: Envelope, keypair: &KeyPair) -> Result<Self, String> {
        envelope.validate(None)?;
        let signature = keypair.sign(&envelope.canonical_json()?)?;
        Ok(Self {
            envelope,
            signature,
        })
    }

    pub fn verify(&self, public_key: &str, recipient: Option<&str>) -> Result<bool, String> {
        self.envelope.validate(recipient)?;
        snartnet_core::verify_signature(
            &self.envelope.canonical_json()?,
            &self.signature,
            public_key,
        )
    }

    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("signed envelope serialization failed: {e}"))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("signed envelope parsing failed: {e}"))?;
        value.envelope.validate(None)?;
        Ok(value)
    }

    pub fn from_signed_message(message: &SignedMessage) -> Self {
        let body = serde_json::Value::String(message.message.content.clone());
        Self {
            envelope: Envelope {
                v: WIRE_VERSION,
                kind: ObjectType::DirectMessage,
                id: message.message.id.clone(),
                from: message.message.sender_fingerprint.clone(),
                to: vec![message.message.recipient_fingerprint.clone()],
                created_at: message.message.created_at,
                expires_at: None,
                body_enc: message.message.body_enc.clone(),
                nonce: message.message.nonce_b64.clone(),
                body,
                prev: None,
                meta: BTreeMap::new(),
            },
            signature: message.signature.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentDescriptor {
    pub magnet: String,
    pub object_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedManifest {
    pub v: u16,
    pub kind: ObjectType,
    pub author: String,
    pub sequence: u64,
    #[serde(default)]
    pub chunks: Vec<TorrentDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxManifest {
    pub v: u16,
    pub kind: ObjectType,
    pub sender: String,
    pub recipient: String,
    pub sequence: u64,
    #[serde(default)]
    pub batches: Vec<TorrentDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<TorrentDescriptor>,
    pub signature: String,
}

impl MailboxManifest {
    pub fn canonical_json(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Unsigned<'a> {
            v: u16,
            kind: &'a ObjectType,
            sender: &'a str,
            recipient: &'a str,
            sequence: u64,
            batches: &'a [TorrentDescriptor],
            previous: &'a Option<TorrentDescriptor>,
        }
        serde_json::to_string(&Unsigned {
            v: self.v,
            kind: &self.kind,
            sender: &self.sender,
            recipient: &self.recipient,
            sequence: self.sequence,
            batches: &self.batches,
            previous: &self.previous,
        })
        .map_err(|e| format!("mailbox manifest serialization failed: {e}"))
    }

    pub fn sign(mut self, keypair: &KeyPair) -> Result<Self, String> {
        self.signature = keypair.sign(&self.canonical_json()?)?;
        Ok(self)
    }

    pub fn verify(&self, public_key: &str, now: DateTime<Utc>) -> Result<bool, String> {
        if self.v != WIRE_VERSION || self.kind != ObjectType::MailboxManifest {
            return Err("unsupported mailbox manifest".into());
        }
        if self.batches.iter().any(|batch| {
            batch
                .expires_at
                .is_some_and(|expires_at| expires_at <= now)
        }) {
            return Err("mailbox manifest contains an expired batch".into());
        }
        snartnet_core::verify_signature(&self.canonical_json()?, &self.signature, public_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtRecord {
    pub key: String,
    pub sequence: u64,
    pub expires_at: DateTime<Utc>,
    pub value: Value,
    pub signer: String,
    pub signature: String,
}

impl DhtRecord {
    pub fn key_for(namespace: &str, parts: &[&str]) -> String {
        let mut hasher = Hasher::new();
        hasher.update(namespace.as_bytes());
        for part in parts {
            hasher.update(&[0]);
            hasher.update(part.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }

    pub fn canonical_json(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Unsigned<'a> {
            key: &'a str,
            sequence: u64,
            expires_at: DateTime<Utc>,
            value: &'a Value,
            signer: &'a str,
        }
        serde_json::to_string(&Unsigned {
            key: &self.key,
            sequence: self.sequence,
            expires_at: self.expires_at,
            value: &self.value,
            signer: &self.signer,
        })
        .map_err(|e| format!("DHT record serialization failed: {e}"))
    }

    pub fn sign(
        namespace: &str,
        parts: &[&str],
        sequence: u64,
        expires_at: DateTime<Utc>,
        value: Value,
        keypair: &KeyPair,
    ) -> Result<Self, String> {
        let mut record = Self {
            key: Self::key_for(namespace, parts),
            sequence,
            expires_at,
            value,
            signer: keypair.fingerprint.clone(),
            signature: String::new(),
        };
        record.signature = keypair.sign(&record.canonical_json()?)?;
        Ok(record)
    }

    pub fn verify(&self, public_key: &str, now: DateTime<Utc>) -> Result<bool, String> {
        if self.expires_at <= now {
            return Err("DHT record has expired".into());
        }
        snartnet_core::verify_signature(&self.canonical_json()?, &self.signature, public_key)
    }
}

pub fn public_post_bytes(post: &SignedPost) -> Result<Vec<u8>, String> {
    serde_json::to_vec(post).map_err(|e| format!("post serialization failed: {e}"))
}

pub fn encode_record(record: &DhtRecord) -> Result<String, String> {
    let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    Ok(STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use snartnet_core::KeyPair;

    #[test]
    fn direct_envelope_is_signed_and_recipient_bound() {
        let keypair = KeyPair::generate().unwrap();
        let envelope = Envelope {
            v: WIRE_VERSION,
            kind: ObjectType::DirectMessage,
            id: "m1".into(),
            from: keypair.fingerprint.clone(),
            to: vec!["bob".into()],
            created_at: Utc::now(),
            expires_at: None,
            body_enc: Some(ENCRYPTION_ALGORITHM.into()),
            nonce: Some("nonce".into()),
            body: Value::String("ciphertext".into()),
            prev: None,
            meta: BTreeMap::new(),
        };
        let signed = SignedEnvelope::sign(envelope, &keypair).unwrap();
        assert!(signed.verify(&keypair.public_key, Some("bob")).unwrap());
        assert!(signed.verify(&keypair.public_key, Some("carol")).is_err());
    }

    #[test]
    fn dht_records_are_expiring_and_signed() {
        let keypair = KeyPair::generate().unwrap();
        let record = DhtRecord::sign(
            "snartnet/mailbox",
            &["alice", "bob"],
            1,
            Utc::now() + Duration::minutes(5),
            serde_json::json!({"magnet":"magnet:?xt=urn:btih:test"}),
            &keypair,
        )
        .unwrap();
        assert!(record.verify(&keypair.public_key, Utc::now()).unwrap());
        assert!(DhtRecord::key_for("snartnet/mailbox", &["alice", "bob"]) == record.key);
    }

    #[test]
    fn mailbox_manifest_is_signed_and_verifies() {
        let keypair = KeyPair::generate().unwrap();
        let now = Utc::now();
        let manifest = MailboxManifest {
            v: WIRE_VERSION,
            kind: ObjectType::MailboxManifest,
            sender: keypair.fingerprint.clone(),
            recipient: "recipient".into(),
            sequence: 1,
            batches: vec![TorrentDescriptor {
                magnet: "magnet:?xt=urn:btih:test".into(),
                object_id: "message-1".into(),
                created_at: now,
                expires_at: Some(now + Duration::hours(1)),
            }],
            previous: None,
            signature: String::new(),
        }
        .sign(&keypair)
        .unwrap();
        assert!(manifest.verify(&keypair.public_key, now).unwrap());
    }
}
