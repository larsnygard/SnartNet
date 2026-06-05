use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, aead::{Aead, KeyInit}};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

const MESSAGE_ENC_ALG: &str = "chacha20poly1305-x25519-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub public_key: String,
    pub secret_key: String,
    pub fingerprint: String,
    #[serde(default)]
    pub enc_public_key: Option<String>,
    #[serde(default)]
    pub enc_secret_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    pub public_key: String,
    pub fingerprint: String,
    #[serde(default)]
    pub encryption_public_key: Option<String>,
}

impl KeyPair {
    pub fn generate() -> Result<Self, String> {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        let public_key = BASE64.encode(verifying_key.as_bytes());
        let secret_key = BASE64.encode(signing_key.as_bytes());

        let enc_secret = StaticSecret::random_from_rng(OsRng);
        let enc_public = X25519PublicKey::from(&enc_secret);
        
        // Create fingerprint from public key hash
        let mut hasher = Sha256::new();
        hasher.update(verifying_key.as_bytes());
        let hash = hasher.finalize();
        let fingerprint = BASE64.encode(&hash[..16]); // First 16 bytes as fingerprint
        
        Ok(KeyPair {
            public_key,
            secret_key,
            fingerprint,
            enc_public_key: Some(BASE64.encode(enc_public.as_bytes())),
            enc_secret_key: Some(BASE64.encode(enc_secret.to_bytes())),
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
            encryption_public_key: self.enc_public_key.clone(),
        }
    }

    pub fn ensure_encryption_keys(&mut self) {
        if self.enc_public_key.is_some() && self.enc_secret_key.is_some() {
            return;
        }

        let enc_secret = StaticSecret::random_from_rng(OsRng);
        let enc_public = X25519PublicKey::from(&enc_secret);
        self.enc_public_key = Some(BASE64.encode(enc_public.as_bytes()));
        self.enc_secret_key = Some(BASE64.encode(enc_secret.to_bytes()));
    }

    pub fn encrypt_for_recipient(
        &self,
        recipient_enc_public_key_b64: &str,
        plaintext: &str,
    ) -> Result<(String, String, String), String> {
        let sender_secret_b64 = self
            .enc_secret_key
            .as_ref()
            .ok_or_else(|| "missing local encryption secret key".to_string())?;
        encrypt_message(sender_secret_b64, recipient_enc_public_key_b64, plaintext)
    }

    pub fn decrypt_from_peer(
        &self,
        peer_enc_public_key_b64: &str,
        nonce_b64: &str,
        ciphertext_b64: &str,
    ) -> Result<String, String> {
        let local_secret_b64 = self
            .enc_secret_key
            .as_ref()
            .ok_or_else(|| "missing local encryption secret key".to_string())?;
        decrypt_message(local_secret_b64, peer_enc_public_key_b64, nonce_b64, ciphertext_b64)
    }
}

pub fn encrypt_message(
    local_secret_b64: &str,
    peer_public_b64: &str,
    plaintext: &str,
) -> Result<(String, String, String), String> {
    let local_secret = decode_32(local_secret_b64, "local encryption secret key")?;
    let peer_public = decode_32(peer_public_b64, "peer encryption public key")?;

    let local_secret = StaticSecret::from(local_secret);
    let peer_public = X25519PublicKey::from(peer_public);
    let shared = local_secret.diffie_hellman(&peer_public);

    let key_bytes = Sha256::digest(shared.as_bytes());
    let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
        .map_err(|e| format!("cipher init failed: {e}"))?;

    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|e| format!("encrypt failed: {e}"))?;

    Ok((
        BASE64.encode(ciphertext),
        BASE64.encode(nonce),
        MESSAGE_ENC_ALG.to_string(),
    ))
}

pub fn decrypt_message(
    local_secret_b64: &str,
    peer_public_b64: &str,
    nonce_b64: &str,
    ciphertext_b64: &str,
) -> Result<String, String> {
    let local_secret = decode_32(local_secret_b64, "local encryption secret key")?;
    let peer_public = decode_32(peer_public_b64, "peer encryption public key")?;
    let nonce = decode_nonce_12(nonce_b64)?;
    let ciphertext = BASE64
        .decode(ciphertext_b64)
        .map_err(|e| format!("ciphertext decode failed: {e}"))?;

    let local_secret = StaticSecret::from(local_secret);
    let peer_public = X25519PublicKey::from(peer_public);
    let shared = local_secret.diffie_hellman(&peer_public);
    let key_bytes = Sha256::digest(shared.as_bytes());
    let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
        .map_err(|e| format!("cipher init failed: {e}"))?;

    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|e| format!("decrypt failed: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| format!("utf8 decode failed: {e}"))
}

fn decode_32(value_b64: &str, label: &str) -> Result<[u8; 32], String> {
    let value = BASE64
        .decode(value_b64)
        .map_err(|e| format!("{label} decode failed: {e}"))?;
    value
        .try_into()
        .map_err(|_| format!("invalid {label} length"))
}

fn decode_nonce_12(value_b64: &str) -> Result<[u8; 12], String> {
    let value = BASE64
        .decode(value_b64)
        .map_err(|e| format!("nonce decode failed: {e}"))?;
    value.try_into().map_err(|_| "invalid nonce length".to_string())
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
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn generate_keypair() -> Result<JsValue, JsValue> {
    let keypair = KeyPair::generate()
        .map_err(|e| JsValue::from_str(&e))?;
    
    serde_wasm_bindgen::to_value(&keypair.get_public_info())
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn sign_data(keypair_json: &str, data: &str) -> Result<String, JsValue> {
    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;
    
    keypair.sign(data)
        .map_err(|e| JsValue::from_str(&e))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn verify_signature_wasm(data: &str, signature: &str, public_key: &str) -> Result<bool, JsValue> {
    verify_signature(data, signature, public_key)
        .map_err(|e| JsValue::from_str(&e))
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