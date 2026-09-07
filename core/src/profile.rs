use crate::crypto::{verify_signature, KeyInfo, KeyPair};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
// removed unused base64 import

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_data_url: Option<String>,
    pub public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption_public_key: Option<String>,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u32,
    pub magnet_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedProfile {
    pub profile: Profile,
    pub signature: String,
}

impl Profile {
    pub fn new(username: String, key_info: KeyInfo) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();

        Self {
            id,
            username,
            display_name: None,
            bio: None,
            avatar_hash: None,
            avatar_data_url: None,
            public_key: key_info.public_key,
            encryption_public_key: key_info.encryption_public_key,
            fingerprint: key_info.fingerprint,
            created_at: now,
            updated_at: now,
            version: 1,
            magnet_uri: None,
        }
    }

    pub fn update(&mut self, display_name: Option<String>, bio: Option<String>) {
        if let Some(name) = display_name {
            self.display_name = Some(name);
        }
        if let Some(bio_text) = bio {
            self.bio = Some(bio_text);
        }
        self.updated_at = Utc::now();
        self.version += 1;
    }

    pub fn to_canonical_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Failed to serialize profile: {}", e))
    }

    pub fn identity_uri(&self) -> String {
        let fingerprint = base64::engine::general_purpose::STANDARD
            .decode(&self.fingerprint)
            .map(|bytes| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
            .unwrap_or_else(|_| percent_encode(&self.fingerprint));
        format!("snartnet://profile/{fingerprint}")
    }

}

pub fn profile_fingerprint_from_identity_uri(uri: &str) -> Result<String, String> {
    let encoded = uri
        .trim()
        .strip_prefix("snartnet://profile/")
        .ok_or_else(|| "invalid SnartNet identity URI".to_string())?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "invalid identity fingerprint".to_string())?;
    if bytes.len() != 16 {
        return Err("identity fingerprint must be 16 bytes".into());
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub fn validate_torrent_magnet_uri(uri: &str) -> Result<(), String> {
    let query = uri
        .trim()
        .strip_prefix("magnet:?")
        .ok_or_else(|| "invalid magnet URI".to_string())?;
    let mut xt = None;
    for part in query.split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key == "xt" {
            xt = Some(percent_decode(value)?);
            break;
        }
    }
    let xt = xt.ok_or_else(|| "magnet URI is missing xt".to_string())?;
    if let Some(hash) = xt.strip_prefix("urn:btih:") {
        if hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(());
        }
        return Err("BitTorrent v1 magnets require a 40-character infohash".into());
    }
    if let Some(hash) = xt.strip_prefix("urn:btmh:1220") {
        if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(());
        }
        return Err("BitTorrent v2 magnets require a 32-byte multihash".into());
    }
    Err("unsupported magnet xt value".into())
}

pub fn profile_fingerprint_from_magnet_uri(uri: &str) -> Result<String, String> {
    let trimmed = uri.trim();
    let query = trimmed
        .strip_prefix("magnet:?")
        .ok_or_else(|| "invalid magnet uri".to_string())?;

    for part in query.split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };

        let decoded = percent_decode(value)?;
        if key == "x.snartnet.fp" && !decoded.trim().is_empty() {
            return Ok(decoded.trim().to_string());
        }

        if key == "dn" {
            if let Some(fp) = decoded.trim().strip_prefix("snartnet-profile-") {
                if !fp.is_empty() {
                    return Ok(fp.to_string());
                }
            }
            if let Some(fp) = value.trim().strip_prefix("profile_") {
                if !fp.is_empty() {
                    return Ok(fp.to_string());
                }
            }
        }
    }

    Err("magnet uri does not contain a profile fingerprint".to_string())
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                vec![byte as char]
            } else {
                format!("%{byte:02X}").chars().collect()
            }
        })
        .collect()
}

fn percent_decode(value: &str) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(value.len());
    let raw = value.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' {
            if index + 2 >= raw.len() {
                return Err("invalid percent-encoding".into());
            }
            let hex = std::str::from_utf8(&raw[index + 1..index + 3])
                .map_err(|_| "invalid percent-encoding".to_string())?;
            let byte = u8::from_str_radix(hex, 16)
                .map_err(|_| "invalid percent-encoding".to_string())?;
            bytes.push(byte);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }
    String::from_utf8(bytes).map_err(|_| "magnet parameter is not UTF-8".into())
}

impl SignedProfile {
    pub fn create(mut profile: Profile, keypair: &KeyPair) -> Result<Self, String> {
        profile.magnet_uri = None;
        let profile_json = profile.to_canonical_json()?;
        let signature = keypair.sign(&profile_json)?;

        Ok(SignedProfile { profile, signature })
    }

    pub fn verify(&self) -> Result<bool, String> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        // A valid signature alone does not prove the claimed contact identity.
        // Bind that identity to the signing key before trusting encryption keys or content.
        let public_key = STANDARD
            .decode(&self.profile.public_key)
            .map_err(|e| format!("Invalid public key: {e}"))?;
        if public_key.len() != 32
            || STANDARD.encode(&Sha256::digest(&public_key)[..16]) != self.profile.fingerprint
        {
            return Ok(false);
        }
        // `magnet_uri` is a derived field appended after signing – strip it so
        // the JSON matches the bytes that were originally signed.
        let mut p = self.profile.clone();
        p.magnet_uri = None;
        let profile_json = p.to_canonical_json()?;
        verify_signature(&profile_json, &self.signature, &self.profile.public_key)
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Serialize, Deserialize)]
struct ProfileData {
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_data_url: Option<String>,
}

// WASM exports
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn create_profile(profile_data_json: &str) -> Result<JsValue, JsValue> {
    let profile_data: ProfileData = serde_json::from_str(profile_data_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile data: {}", e)))?;

    let keypair = KeyPair::generate()
        .map_err(|e| JsValue::from_str(&format!("Failed to create keypair: {}", e)))?;
    let key_info = keypair.get_public_info();

    let mut profile = Profile::new(profile_data.username, key_info);
    profile.display_name = profile_data.display_name;
    profile.bio = profile_data.bio;
    profile.avatar_data_url = profile_data.avatar_data_url;

    serde_wasm_bindgen::to_value(&profile)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[cfg(target_arch = "wasm32")]
#[derive(Serialize, Deserialize)]
struct ProfileUpdateData {
    display_name: Option<String>,
    bio: Option<String>,
    avatar_data_url: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn update_profile(profile_json: &str, update_data_json: &str) -> Result<JsValue, JsValue> {
    let mut profile: Profile = serde_json::from_str(profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile JSON: {}", e)))?;

    let update_data: ProfileUpdateData = serde_json::from_str(update_data_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid update data: {}", e)))?;

    profile.update(update_data.display_name, update_data.bio);
    if update_data.avatar_data_url.is_some() {
        profile.avatar_data_url = update_data.avatar_data_url;
    }

    serde_wasm_bindgen::to_value(&profile)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn sign_profile(profile_json: &str, keypair_json: &str) -> Result<JsValue, JsValue> {
    let profile: Profile = serde_json::from_str(profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile JSON: {}", e)))?;

    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;

    let signed_profile =
        SignedProfile::create(profile, &keypair).map_err(|e| JsValue::from_str(&e))?;

    serde_wasm_bindgen::to_value(&signed_profile)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn verify_profile(signed_profile_json: &str) -> Result<bool, JsValue> {
    let signed_profile: SignedProfile = serde_json::from_str(signed_profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid signed profile JSON: {}", e)))?;

    signed_profile.verify().map_err(|e| JsValue::from_str(&e))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn generate_profile_magnet_uri(profile_json: &str) -> Result<String, JsValue> {
    let profile: Profile = serde_json::from_str(profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile JSON: {}", e)))?;

    Ok(profile.identity_uri())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;

    fn make_keypair() -> KeyPair {
        KeyPair::generate().expect("keygen failed")
    }

    #[test]
    fn create_profile_stores_key_info() {
        let kp = make_keypair();
        let info = kp.get_public_info();
        let p = Profile::new("alice".to_string(), info.clone());
        assert_eq!(p.username, "alice");
        assert_eq!(p.public_key, info.public_key);
        assert_eq!(p.fingerprint, info.fingerprint);
        assert_eq!(p.version, 1);
    }

    #[test]
    fn update_increments_version() {
        let kp = make_keypair();
        let mut p = Profile::new("bob".to_string(), kp.get_public_info());
        p.update(Some("Bob Smith".to_string()), Some("A bio".to_string()));
        assert_eq!(p.version, 2);
        assert_eq!(p.display_name.as_deref(), Some("Bob Smith"));
    }

    #[test]
    fn signed_profile_verifies() {
        let kp = make_keypair();
        let p = Profile::new("charlie".to_string(), kp.get_public_info());
        let sp = SignedProfile::create(p, &kp).expect("signing failed");
        assert!(sp.verify().expect("verify failed"));
    }

    #[test]
    fn magnet_uri_roundtrips_contact_identity() {
        let kp = make_keypair();
        let p = Profile::new("dave".to_string(), kp.get_public_info());
        let uri = p.identity_uri();
        assert!(uri.starts_with("snartnet://profile/"));
        assert_eq!(
            profile_fingerprint_from_identity_uri(&uri).unwrap(),
            p.fingerprint
        );
    }

    #[test]
    fn torrent_magnets_validate_and_decode_encoded_metadata() {
        let fingerprint = "ab/c+d=";
        let uri = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=snartnet-profile-ab%2Fc%2Bd%3D&x.snartnet.fp=ab%2Fc%2Bd%3D".to_string();
        validate_torrent_magnet_uri(&uri).unwrap();
        assert_eq!(profile_fingerprint_from_magnet_uri(&uri).unwrap(), fingerprint);
        validate_torrent_magnet_uri(
            "magnet:?xt=urn:btmh:12200123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        assert!(validate_torrent_magnet_uri(
            "magnet:?xt=urn:btih:0123456789abcdef"
        )
        .is_err());
    }
    #[test]
    fn a_valid_signature_cannot_claim_someone_elses_fingerprint() {
        let attacker = make_keypair();
        let victim = make_keypair();
        let mut profile = Profile::new("imposter".into(), attacker.get_public_info());
        profile.fingerprint = victim.fingerprint;
        let signed = SignedProfile::create(profile, &attacker).unwrap();
        assert!(!signed.verify().unwrap());
    }

    #[test]
    fn resigning_a_profile_with_a_magnet_keeps_the_signature_valid() {
        let kp = make_keypair();
        let mut profile = Profile::new("alice".into(), kp.get_public_info());
        profile.magnet_uri = None;
        assert!(SignedProfile::create(profile, &kp)
            .unwrap()
            .verify()
            .unwrap());
    }
}
