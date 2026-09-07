//! Shared native client protocol, records and validated actions.
pub mod actions;
pub mod dht;
pub mod discovery;
pub mod model;
pub mod protocol;
pub mod session;
pub mod sync;
pub mod torrent;
pub mod transport;
use base64::{engine::general_purpose, Engine as _};
use model::*;
use snartnet_core::{
    profile_fingerprint_from_magnet_uri, ContactInvite, FileStorage, KeyPair,
    Message as CoreMessage, Post, Profile, SignedMessage, SignedPost, SignedProfile,
};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};
use transport::{NetworkTransport, TcpSwarmTransport};
const STORAGE_KEYPAIR: &str = "keypair";
const STORAGE_PROFILE: &str = "profile";
const STORAGE_POSTS: &str = "local_posts";
const STORAGE_CONTACTS: &str = "contacts";
const STORAGE_THREADS: &str = "threads";
fn default_trust() -> u8 {
    20
}
fn short_fp(fp: &str) -> String {
    fp.chars().take(8).collect()
}
fn ts_label() -> String {
    chrono::Utc::now().format("%H:%M:%S UTC").to_string()
}
fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn decrypt_for_display(
    item: &ChatItem,
    keypair: Option<&KeyPair>,
    peer_enc_public_key: Option<&str>,
) -> Result<String, String> {
    if !item.encrypted {
        return Ok(item.content.clone());
    }

    if !item.verified_sender {
        return Err("sender signature not verified".to_string());
    }

    if item.encryption_alg.as_deref() != Some("chacha20poly1305-x25519-v1") {
        return Err("unsupported encryption format".into());
    }
    let kp = keypair.ok_or_else(|| "missing local keypair".to_string())?;
    let peer_key = item
        .peer_encryption_key
        .as_deref()
        .or(peer_enc_public_key)
        .ok_or_else(|| "missing peer encryption key".to_string())?;
    let nonce = item
        .nonce_b64
        .as_deref()
        .ok_or_else(|| "missing nonce".to_string())?;
    kp.decrypt_from_peer(peer_key, nonce, &item.content)
}
