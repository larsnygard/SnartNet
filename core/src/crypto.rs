use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub public_key: String,
    pub secret_key: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    pub public_key: String,
    pub fingerprint: String,
}

impl KeyPair {
    pub fn generate() -> Result<Self, String> {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        let public_key = BASE64.encode(verifying_key.as_bytes());
        let secret_key = BASE64.encode(signing_key.as_bytes());
        
        // Create fingerprint from public key hash
        let mut hasher = Sha256::new();
        hasher.update(verifying_key.as_bytes());
        let hash = hasher.finalize();
        let fingerprint = BASE64.encode(&hash[..16]); // First 16 bytes as fingerprint
        
        Ok(KeyPair {
            public_key,
            secret_key,
            fingerprint,
        })
    }
    
    pub fn sign(&self, data: &str) -> Result<String, String> {
        let secret_bytes = BASE64.decode(&self.secret_key)
            .map_err(|e| format!("Failed to decode secret key: {}", e))?;
        
        let secret_array: [u8; 32] = secret_bytes.try_into()
            .map_err(|_| "Invalid secret key length")?;
        
        let signing_key = SigningKey::from_bytes(&secret_array);
        let signature = signing_key.sign(data.as_bytes());
        Ok(BASE64.encode(signature.to_bytes()))
    }
    
    pub fn get_public_info(&self) -> KeyInfo {
        KeyInfo {
            public_key: self.public_key.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

pub fn verify_signature(data: &str, signature: &str, public_key: &str) -> Result<bool, String> {
    let public_bytes = BASE64.decode(public_key)
        .map_err(|e| format!("Failed to decode public key: {}", e))?;
    
    let public_array: [u8; 32] = public_bytes.try_into()
        .map_err(|_| "Invalid public key length")?;
    
    let verifying_key = VerifyingKey::from_bytes(&public_array)
        .map_err(|e| format!("Invalid public key: {}", e))?;
    
    let sig_bytes = BASE64.decode(signature)
        .map_err(|e| format!("Failed to decode signature: {}", e))?;
    
    let sig_array: [u8; 64] = sig_bytes.try_into()
        .map_err(|_| "Invalid signature length")?;
    
    let signature = Signature::from_bytes(&sig_array);
    
    match verifying_key.verify(data.as_bytes(), &signature) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

// WASM exports
#[wasm_bindgen]
pub fn generate_keypair() -> Result<JsValue, JsValue> {
    let keypair = KeyPair::generate()
        .map_err(|e| JsValue::from_str(&e))?;
    
    serde_wasm_bindgen::to_value(&keypair.get_public_info())
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn sign_data(keypair_json: &str, data: &str) -> Result<String, JsValue> {
    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;
    
    keypair.sign(data)
        .map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn verify_signature_wasm(data: &str, signature: &str, public_key: &str) -> Result<bool, JsValue> {
    verify_signature(data, signature, public_key)
        .map_err(|e| JsValue::from_str(&e))
}