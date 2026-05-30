use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

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
        let secret_bytes = BASE64
            .decode(&self.secret_key)
            .map_err(|e| format!("Failed to decode secret key: {}", e))?;

        let secret_array: [u8; 32] = secret_bytes
            .try_into()
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
    let public_bytes = BASE64
        .decode(public_key)
        .map_err(|e| format!("Failed to decode public key: {}", e))?;

    let public_array: [u8; 32] = public_bytes
        .try_into()
        .map_err(|_| "Invalid public key length")?;

    let verifying_key = VerifyingKey::from_bytes(&public_array)
        .map_err(|e| format!("Invalid public key: {}", e))?;

    let sig_bytes = BASE64
        .decode(signature)
        .map_err(|e| format!("Failed to decode signature: {}", e))?;

    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "Invalid signature length")?;

    let signature = Signature::from_bytes(&sig_array);

    match verifying_key.verify(data.as_bytes(), &signature) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

// WASM exports
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn generate_keypair() -> Result<JsValue, JsValue> {
    let keypair = KeyPair::generate().map_err(|e| JsValue::from_str(&e))?;

    serde_wasm_bindgen::to_value(&keypair.get_public_info())
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn sign_data(keypair_json: &str, data: &str) -> Result<String, JsValue> {
    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;

    keypair.sign(data).map_err(|e| JsValue::from_str(&e))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn verify_signature_wasm(
    data: &str,
    signature: &str,
    public_key: &str,
) -> Result<bool, JsValue> {
    verify_signature(data, signature, public_key).map_err(|e| JsValue::from_str(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generate_produces_unique_keys() {
        let kp1 = KeyPair::generate().expect("first keygen failed");
        let kp2 = KeyPair::generate().expect("second keygen failed");
        assert_ne!(kp1.public_key, kp2.public_key);
        assert_ne!(kp1.fingerprint, kp2.fingerprint);
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let kp = KeyPair::generate().expect("keygen failed");
        let data = "hello snartnet";
        let sig = kp.sign(data).expect("sign failed");
        let valid = verify_signature(data, &sig, &kp.public_key).expect("verify failed");
        assert!(valid, "signature should be valid");
    }

    #[test]
    fn verify_rejects_tampered_data() {
        let kp = KeyPair::generate().expect("keygen failed");
        let sig = kp.sign("original").expect("sign failed");
        let valid = verify_signature("tampered", &sig, &kp.public_key).expect("verify failed");
        assert!(!valid, "signature should be invalid for tampered data");
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let kp1 = KeyPair::generate().expect("keygen failed");
        let kp2 = KeyPair::generate().expect("keygen failed");
        let sig = kp1.sign("data").expect("sign failed");
        let valid = verify_signature("data", &sig, &kp2.public_key).expect("verify failed");
        assert!(!valid, "signature should be invalid for wrong key");
    }

    #[test]
    fn get_public_info_hides_secret_key() {
        let kp = KeyPair::generate().expect("keygen failed");
        let info = kp.get_public_info();
        assert_eq!(info.public_key, kp.public_key);
        assert_eq!(info.fingerprint, kp.fingerprint);
    }
}
