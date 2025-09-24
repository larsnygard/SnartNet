use crate::crypto::{KeyPair, verify_signature};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender_fingerprint: String,
    pub recipient_fingerprint: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub encrypted: bool,
    pub message_type: MessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Direct,
    Group { group_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    pub message: Message,
    pub signature: String,
}

impl Message {
    pub fn new_direct(
        sender_fingerprint: String,
        recipient_fingerprint: String,
        content: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            sender_fingerprint,
            recipient_fingerprint,
            content,
            created_at: Utc::now(),
            encrypted: false, // TODO: Implement encryption
            message_type: MessageType::Direct,
        }
    }
    
    pub fn new_group(
        sender_fingerprint: String,
        recipient_fingerprint: String,
        group_id: String,
        content: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            sender_fingerprint,
            recipient_fingerprint,
            content,
            created_at: Utc::now(),
            encrypted: false, // TODO: Implement encryption
            message_type: MessageType::Group { group_id },
        }
    }
    
    pub fn to_canonical_json(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize message: {}", e))
    }
}

impl SignedMessage {
    pub fn create(message: Message, keypair: &KeyPair) -> Result<Self, String> {
        let message_json = message.to_canonical_json()?;
        let signature = keypair.sign(&message_json)?;
        
        Ok(SignedMessage {
            message,
            signature,
        })
    }
    
    pub fn verify(&self, public_key: &str) -> Result<bool, String> {
        let message_json = self.message.to_canonical_json()?;
        verify_signature(&message_json, &self.signature, public_key)
    }
}

// WASM exports
#[wasm_bindgen]
pub fn create_direct_message(
    sender_fingerprint: &str,
    recipient_fingerprint: &str,
    content: &str,
) -> Result<JsValue, JsValue> {
    let message = Message::new_direct(
        sender_fingerprint.to_string(),
        recipient_fingerprint.to_string(),
        content.to_string(),
    );
    
    serde_wasm_bindgen::to_value(&message)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn sign_message(message_json: &str, keypair_json: &str) -> Result<JsValue, JsValue> {
    let message: Message = serde_json::from_str(message_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid message JSON: {}", e)))?;
    
    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;
    
    let signed_message = SignedMessage::create(message, &keypair)
        .map_err(|e| JsValue::from_str(&e))?;
    
    serde_wasm_bindgen::to_value(&signed_message)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn verify_message(signed_message_json: &str, public_key: &str) -> Result<bool, JsValue> {
    let signed_message: SignedMessage = serde_json::from_str(signed_message_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid signed message JSON: {}", e)))?;
    
    signed_message.verify(public_key)
        .map_err(|e| JsValue::from_str(&e))
}