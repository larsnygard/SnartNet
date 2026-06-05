use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

use crate::profile::SignedProfile;

const COMPRESSED_INVITE_PREFIX: &str = "z1_";

/// A shareable invite payload that bundles everything a peer needs to add you
/// as a contact and start syncing your profile/feed.
///
/// This is a distribution wrapper around the signed profile identity; the
/// signed profile remains the authoritative source of truth.  The invite is
/// intentionally compact so it can be encoded as a QR code or shared via
/// local-network broadcast.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContactInvite {
    /// Ed25519 public-key fingerprint – the durable identity anchor.
    pub fingerprint: String,
    /// Username from the profile.
    pub username: String,
    /// Optional human-readable display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Magnet URI pointing at the profile torrent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnet_uri: Option<String>,
    /// Optional direct TCP transport hint (`"host:port"`) for LAN delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_addr: Option<String>,
}

impl ContactInvite {
    /// Build an invite from a signed profile, optionally including a direct
    /// transport address for LAN delivery.
    pub fn from_signed_profile(sp: &SignedProfile, transport_addr: Option<String>) -> Self {
        Self {
            fingerprint: sp.profile.fingerprint.clone(),
            username: sp.profile.username.clone(),
            display_name: sp.profile.display_name.clone(),
            magnet_uri: sp.profile.magnet_uri.clone(),
            transport_addr,
        }
    }

    /// Serialize to compact JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("invite serialize failed: {e}"))
    }

    /// Deserialize from JSON.
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("invite parse failed: {e}"))
    }

    /// Encode as URL-safe base64 (no padding) for use in QR codes or text sharing.
    pub fn to_base64(&self) -> Result<String, String> {
        let json = self.to_json()?;
        let mut encoder = flate2::write::ZlibEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        );
        encoder
            .write_all(json.as_bytes())
            .map_err(|e| format!("invite compression failed: {e}"))?;
        let compressed = encoder
            .finish()
            .map_err(|e| format!("invite compression finalize failed: {e}"))?;
        Ok(format!(
            "{COMPRESSED_INVITE_PREFIX}{}",
            general_purpose::URL_SAFE_NO_PAD.encode(compressed)
        ))
    }

    /// Decode from URL-safe base64 produced by [`to_base64`].
    pub fn from_base64(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if let Some(payload) = trimmed.strip_prefix(COMPRESSED_INVITE_PREFIX) {
            let compressed = general_purpose::URL_SAFE_NO_PAD
                .decode(payload)
                .map_err(|e| format!("base64 decode failed: {e}"))?;
            let mut decoder = flate2::read::ZlibDecoder::new(compressed.as_slice());
            let mut json = String::new();
            decoder
                .read_to_string(&mut json)
                .map_err(|e| format!("invite decompression failed: {e}"))?;
            return Self::from_json(&json);
        }

        // Backward compatibility: accept legacy base64(JSON) invite format.
        let bytes = general_purpose::URL_SAFE_NO_PAD
            .decode(trimmed)
            .map_err(|e| format!("base64 decode failed: {e}"))?;
        let json = std::str::from_utf8(&bytes).map_err(|e| format!("utf8 decode failed: {e}"))?;
        Self::from_json(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;
    use crate::profile::{Profile, SignedProfile};

    fn make_signed_profile(username: &str) -> SignedProfile {
        let kp = KeyPair::generate().expect("keygen");
        let mut p = Profile::new(username.to_string(), kp.get_public_info());
        p.display_name = Some("Test User".to_string());
        p.magnet_uri = Some(p.generate_magnet_uri());
        SignedProfile::create(p, &kp).expect("sign")
    }

    #[test]
    fn invite_from_signed_profile() {
        let sp = make_signed_profile("alice");
        let invite = ContactInvite::from_signed_profile(&sp, None);
        assert_eq!(invite.fingerprint, sp.profile.fingerprint);
        assert_eq!(invite.username, "alice");
        assert_eq!(invite.display_name.as_deref(), Some("Test User"));
        assert_eq!(invite.magnet_uri, sp.profile.magnet_uri);
        assert!(invite.transport_addr.is_none());
    }

    #[test]
    fn invite_with_transport_addr() {
        let sp = make_signed_profile("bob");
        let invite =
            ContactInvite::from_signed_profile(&sp, Some("192.168.1.5:47470".to_string()));
        assert_eq!(invite.transport_addr.as_deref(), Some("192.168.1.5:47470"));
    }

    #[test]
    fn invite_json_roundtrip() {
        let sp = make_signed_profile("carol");
        let invite =
            ContactInvite::from_signed_profile(&sp, Some("10.0.0.1:47470".to_string()));
        let json = invite.to_json().expect("to_json");
        let restored = ContactInvite::from_json(&json).expect("from_json");
        assert_eq!(restored, invite);
    }

    #[test]
    fn invite_base64_roundtrip() {
        let sp = make_signed_profile("dave");
        let invite = ContactInvite::from_signed_profile(&sp, None);
        let encoded = invite.to_base64().expect("to_base64");
        assert!(
            encoded.starts_with(COMPRESSED_INVITE_PREFIX),
            "encoded invite should include compressed format prefix"
        );
        // URL_SAFE_NO_PAD must not contain '=' padding
        assert!(!encoded.contains('='), "URL_SAFE_NO_PAD should have no padding");
        // Only URL-safe characters
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "encoded string must be URL-safe"
        );
        let restored = ContactInvite::from_base64(&encoded).expect("from_base64");
        assert_eq!(restored, invite);
    }

    #[test]
    fn invite_base64_legacy_roundtrip() {
        let sp = make_signed_profile("legacy");
        let invite = ContactInvite::from_signed_profile(&sp, None);
        let json = invite.to_json().expect("json");
        let legacy = general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
        let restored = ContactInvite::from_base64(&legacy).expect("legacy decode");
        assert_eq!(restored, invite);
    }

    #[test]
    fn invite_base64_compressed_is_shorter_than_legacy() {
        let sp = make_signed_profile("compressed");
        let invite =
            ContactInvite::from_signed_profile(&sp, Some("192.168.1.5:47470".to_string()));
        let json = invite.to_json().expect("json");
        let legacy = general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
        let compressed = invite.to_base64().expect("compressed");
        assert!(
            compressed.len() < legacy.len(),
            "compressed invite should be shorter than legacy format"
        );
    }

    #[test]
    fn invite_base64_trims_whitespace() {
        let sp = make_signed_profile("eve");
        let invite = ContactInvite::from_signed_profile(&sp, None);
        let encoded = format!("  {}  \n", invite.to_base64().unwrap());
        let restored = ContactInvite::from_base64(&encoded).expect("trim + decode");
        assert_eq!(restored.fingerprint, invite.fingerprint);
    }

    #[test]
    fn invite_without_optional_fields() {
        let kp = KeyPair::generate().expect("keygen");
        let p = Profile::new("frank".to_string(), kp.get_public_info());
        let sp = SignedProfile::create(p, &kp).expect("sign");
        let invite = ContactInvite::from_signed_profile(&sp, None);
        assert!(invite.magnet_uri.is_none());
        assert!(invite.transport_addr.is_none());
        assert!(invite.display_name.is_none());
        // Round-trip still works; optional fields are absent from the serialized form.
        let enc = invite.to_base64().expect("encode");
        let dec = ContactInvite::from_base64(&enc).expect("decode");
        assert_eq!(dec.fingerprint, invite.fingerprint);
        assert_eq!(dec.username, "frank");
        assert!(dec.magnet_uri.is_none());
    }

    #[test]
    fn invite_base64_rejects_garbage() {
        assert!(ContactInvite::from_base64("not-valid-base64!!!").is_err());
    }

    #[test]
    fn invite_json_rejects_garbage() {
        assert!(ContactInvite::from_json("not json").is_err());
    }
}
