use wasm_bindgen::prelude::*;
use crate::crypto::KeyPair;
use crate::profile::{Profile, SignedProfile};
use crate::post::{Post, SignedPost};
use crate::message::{Message, SignedMessage};
use crate::storage::LocalStorage;
use serde::{Serialize, Deserialize};

// ----- Shared JSON API structs (additive, forward-compatible) -----
#[derive(Serialize, Deserialize)]
struct CreateProfileRequest {
    username: String,
    #[serde(rename = "displayName")] display_name: Option<String>,
    bio: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct UpdateProfileRequest {
    #[serde(rename = "displayName")] display_name: Option<String>,
    bio: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ProfileEnvelope {
    profile: Profile,
    signature: String,
    #[serde(rename = "magnetUri")] magnet_uri: String,
    api: String,
    version: u32,
}

#[derive(Serialize, Deserialize)]
struct CapabilityDescriptor {
    #[serde(rename = "profileJsonApi")] profile_json_api: bool,
    #[serde(rename = "postJsonApi")] post_json_api: bool,
    #[serde(rename = "messageJsonApi")] message_json_api: bool,
    version: String,
}

// High-level WASM interface that matches the TypeScript interface
#[wasm_bindgen]
pub struct SnartNetCore {
    current_profile: Option<SignedProfile>,
    keypair: Option<KeyPair>,
}

#[wasm_bindgen]
impl SnartNetCore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            current_profile: None,
            keypair: None,
        }
    }
    
    // Initialize core
    #[wasm_bindgen]
    pub fn init(&mut self) -> Result<(), JsValue> {
        web_sys::console::log_1(&"SnartNet Core WASM initialized".into());
        
        // Try to load existing profile and keypair from storage
        if let Ok(Some(keypair)) = LocalStorage::get_json::<KeyPair>("snartnet_keypair") {
            self.keypair = Some(keypair);
        }
        
        if let Ok(Some(profile)) = LocalStorage::get_json::<SignedProfile>("snartnet_current_profile") {
            self.current_profile = Some(profile);
        }
        
        Ok(())
    }
    
    // Profile management
    #[wasm_bindgen]
    pub fn create_profile(&mut self, username: &str, display_name: Option<String>, bio: Option<String>) -> Result<String, JsValue> {
        // Generate keypair if we don't have one
        if self.keypair.is_none() {
            self.keypair = Some(KeyPair::generate()
                .map_err(|e| JsValue::from_str(&e))?);
        }
        
        let keypair = self.keypair.as_ref().unwrap();
        
        // Create profile
        let mut profile = Profile::new(username.to_string(), keypair.get_public_info());
        profile.update(display_name, bio);
        
        // Sign the profile
        let signed_profile = SignedProfile::create(profile, keypair)
            .map_err(|e| JsValue::from_str(&e))?;
        
    let magnet_uri = signed_profile.profile.generate_magnet_uri();
    // Store magnet URI inside profile for downstream consumers
    let mut signed_profile = signed_profile;
    signed_profile.profile.magnet_uri = Some(magnet_uri.clone());
        
        // Store keypair and profile
        LocalStorage::set_json("snartnet_keypair", keypair)?;
        LocalStorage::set_json("snartnet_current_profile", &signed_profile)?;
        
    self.current_profile = Some(signed_profile);
        
        Ok(magnet_uri)
    }
    
    #[wasm_bindgen]
    pub fn get_current_profile(&self) -> Result<JsValue, JsValue> {
        match &self.current_profile {
            Some(profile) => serde_wasm_bindgen::to_value(&profile.profile)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e))),
            None => Ok(JsValue::null()),
        }
    }
    
    #[wasm_bindgen]
    pub fn update_current_profile(&mut self, display_name: Option<String>, bio: Option<String>) -> Result<(), JsValue> {
        if let (Some(profile), Some(keypair)) = (&mut self.current_profile, &self.keypair) {
            profile.profile.update(display_name, bio);
            
            // Re-sign the profile
            let new_signed_profile = SignedProfile::create(profile.profile.clone(), keypair)
                .map_err(|e| JsValue::from_str(&e))?;
            let magnet_uri = new_signed_profile.profile.generate_magnet_uri();
            let mut new_signed_profile = new_signed_profile;
            new_signed_profile.profile.magnet_uri = Some(magnet_uri);
            
            // Store updated profile
            LocalStorage::set_json("snartnet_current_profile", &new_signed_profile)?;
            self.current_profile = Some(new_signed_profile);
            
            Ok(())
        } else {
            Err(JsValue::from_str("No current profile"))
        }
    }

    // --- Additive JSON-based API (forward-compatible) ---
    #[wasm_bindgen]
    pub fn create_profile_json(&mut self, payload_json: &str) -> Result<JsValue, JsValue> {
        let req: CreateProfileRequest = serde_json::from_str(payload_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid create_profile_json payload: {}", e)))?;
        let magnet = self.create_profile(&req.username, req.display_name.clone(), req.bio.clone())?;
        // We just created it, so unwrap is safe
        if let Some(ref signed) = self.current_profile {
            let env = ProfileEnvelope {
                profile: signed.profile.clone(),
                signature: signed.signature.clone(),
                magnet_uri: magnet.clone(),
                api: "profile-json-v1".to_string(),
                version: signed.profile.version,
            };
            return serde_wasm_bindgen::to_value(&env)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)));
        }
        Err(JsValue::from_str("Profile creation failed unexpectedly"))
    }

    #[wasm_bindgen]
    pub fn update_profile_json(&mut self, payload_json: &str) -> Result<JsValue, JsValue> {
        let req: UpdateProfileRequest = serde_json::from_str(payload_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid update_profile_json payload: {}", e)))?;
        self.update_current_profile(req.display_name.clone(), req.bio.clone())?;
        if let Some(ref signed) = self.current_profile {
            let magnet_uri = signed.profile.magnet_uri.clone().unwrap_or_else(|| signed.profile.generate_magnet_uri());
            let env = ProfileEnvelope {
                profile: signed.profile.clone(),
                signature: signed.signature.clone(),
                magnet_uri,
                api: "profile-json-v1".to_string(),
                version: signed.profile.version,
            };
            return serde_wasm_bindgen::to_value(&env)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)));
        }
        Err(JsValue::from_str("No current profile"))
    }

    #[wasm_bindgen]
    pub fn get_current_profile_json(&self) -> Result<JsValue, JsValue> {
        if let Some(ref signed) = self.current_profile {
            let magnet_uri = signed.profile.magnet_uri.clone().unwrap_or_else(|| signed.profile.generate_magnet_uri());
            let env = ProfileEnvelope {
                profile: signed.profile.clone(),
                signature: signed.signature.clone(),
                magnet_uri,
                api: "profile-json-v1".to_string(),
                version: signed.profile.version,
            };
            return serde_wasm_bindgen::to_value(&env)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)));
        }
        Ok(JsValue::null())
    }

    #[wasm_bindgen]
    pub fn get_capabilities(&self) -> Result<JsValue, JsValue> {
        let caps = CapabilityDescriptor {
            profile_json_api: true,
            post_json_api: false,
            message_json_api: false,
            version: "1".to_string(),
        };
        serde_wasm_bindgen::to_value(&caps)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }
    
    // Post management
    #[wasm_bindgen]
    pub fn create_post(&self, content: &str, tags: Option<Vec<String>>, reply_to: Option<String>) -> Result<JsValue, JsValue> {
        let keypair = self.keypair.as_ref()
            .ok_or_else(|| JsValue::from_str("No keypair available"))?;
        
        let profile = self.current_profile.as_ref()
            .ok_or_else(|| JsValue::from_str("No current profile"))?;
        
        // Create post
        let post = Post::new(
            profile.profile.fingerprint.clone(),
            content.to_string(),
            tags,
            reply_to,
        );
        
        // Sign the post
        let signed_post = SignedPost::create(post, keypair)
            .map_err(|e| JsValue::from_str(&e))?;
        
        serde_wasm_bindgen::to_value(&signed_post)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }
    
    // Message management
    #[wasm_bindgen]
    pub fn create_message(&self, recipient_fingerprint: &str, content: &str) -> Result<JsValue, JsValue> {
        let keypair = self.keypair.as_ref()
            .ok_or_else(|| JsValue::from_str("No keypair available"))?;
        
        let profile = self.current_profile.as_ref()
            .ok_or_else(|| JsValue::from_str("No current profile"))?;
        
        // Create message
        let message = Message::new_direct(
            profile.profile.fingerprint.clone(),
            recipient_fingerprint.to_string(),
            content.to_string(),
        );
        
        // Sign the message
        let signed_message = SignedMessage::create(message, keypair)
            .map_err(|e| JsValue::from_str(&e))?;
        
        serde_wasm_bindgen::to_value(&signed_message)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }
    
    // Utility functions
    #[wasm_bindgen]
    pub fn get_public_key(&self) -> Result<String, JsValue> {
        match &self.keypair {
            Some(keypair) => Ok(keypair.public_key.clone()),
            None => Err(JsValue::from_str("No keypair available")),
        }
    }
    
    #[wasm_bindgen]
    pub fn get_fingerprint(&self) -> Result<String, JsValue> {
        match &self.keypair {
            Some(keypair) => Ok(keypair.fingerprint.clone()),
            None => Err(JsValue::from_str("No keypair available")),
        }
    }
    
    #[wasm_bindgen]
    pub fn has_profile(&self) -> bool {
        self.current_profile.is_some()
    }
}