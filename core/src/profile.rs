use crate::crypto::{KeyPair, KeyInfo, verify_signature};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wasm_bindgen::prelude::*;
// removed unused base64 import

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_hash: Option<String>,
    pub public_key: String,
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
            public_key: key_info.public_key,
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
        serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize profile: {}", e))
    }
    
    pub fn generate_magnet_uri(&self) -> String {
        // Generate a deterministic hash for the profile
        let json = self.to_canonical_json().unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);
        
        format!("magnet:?xt=urn:btih:{}&dn=profile_{}", hash_hex, self.username)
    }
}

impl SignedProfile {
    pub fn create(profile: Profile, keypair: &KeyPair) -> Result<Self, String> {
        let profile_json = profile.to_canonical_json()?;
        let signature = keypair.sign(&profile_json)?;
        
        Ok(SignedProfile {
            profile,
            signature,
        })
    }
    
    pub fn verify(&self) -> Result<bool, String> {
        let profile_json = self.profile.to_canonical_json()?;
        verify_signature(&profile_json, &self.signature, &self.profile.public_key)
    }
}

#[derive(Serialize, Deserialize)]
struct ProfileData {
    username: String,
    displayName: Option<String>,
    bio: Option<String>,
}

// WASM exports
#[wasm_bindgen]
pub fn create_profile(profile_data_json: &str) -> Result<JsValue, JsValue> {
    let profile_data: ProfileData = serde_json::from_str(profile_data_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile data: {}", e)))?;

    let keypair = KeyPair::generate()
        .map_err(|e| JsValue::from_str(&format!("Failed to create keypair: {}", e)))?;
    let key_info = keypair.get_public_info();
    
    let mut profile = Profile::new(profile_data.username, key_info);
    profile.display_name = profile_data.displayName;
    profile.bio = profile_data.bio;
    
    serde_wasm_bindgen::to_value(&profile)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[derive(Serialize, Deserialize)]
struct ProfileUpdateData {
    displayName: Option<String>,
    bio: Option<String>,
}

#[wasm_bindgen]
pub fn update_profile(profile_json: &str, update_data_json: &str) -> Result<JsValue, JsValue> {
    let mut profile: Profile = serde_json::from_str(profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile JSON: {}", e)))?;
    
    let update_data: ProfileUpdateData = serde_json::from_str(update_data_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid update data: {}", e)))?;

    profile.update(update_data.displayName, update_data.bio);
    
    serde_wasm_bindgen::to_value(&profile)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn sign_profile(profile_json: &str, keypair_json: &str) -> Result<JsValue, JsValue> {
    let profile: Profile = serde_json::from_str(profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile JSON: {}", e)))?;
    
    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;
    
    let signed_profile = SignedProfile::create(profile, &keypair)
        .map_err(|e| JsValue::from_str(&e))?;
    
    serde_wasm_bindgen::to_value(&signed_profile)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn verify_profile(signed_profile_json: &str) -> Result<bool, JsValue> {
    let signed_profile: SignedProfile = serde_json::from_str(signed_profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid signed profile JSON: {}", e)))?;
    
    signed_profile.verify()
        .map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn generate_profile_magnet_uri(profile_json: &str) -> Result<String, JsValue> {
    let profile: Profile = serde_json::from_str(profile_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid profile JSON: {}", e)))?;
    
    Ok(profile.generate_magnet_uri())
}