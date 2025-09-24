use crate::crypto::{KeyPair, verify_signature};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub author_fingerprint: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub reply_to: Option<String>,
    pub attachment_hashes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPost {
    pub post: Post,
    pub signature: String,
}

impl Post {
    pub fn new(
        author_fingerprint: String,
        content: String,
        tags: Option<Vec<String>>,
        reply_to: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            author_fingerprint,
            content,
            tags: tags.unwrap_or_default(),
            created_at: Utc::now(),
            reply_to,
            attachment_hashes: Vec::new(),
        }
    }
    
    pub fn to_canonical_json(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize post: {}", e))
    }
    
    pub fn add_attachment(&mut self, hash: String) {
        self.attachment_hashes.push(hash);
    }
}

impl SignedPost {
    pub fn create(post: Post, keypair: &KeyPair) -> Result<Self, String> {
        let post_json = post.to_canonical_json()?;
        let signature = keypair.sign(&post_json)?;
        
        Ok(SignedPost {
            post,
            signature,
        })
    }
    
    pub fn verify(&self, public_key: &str) -> Result<bool, String> {
        let post_json = self.post.to_canonical_json()?;
        verify_signature(&post_json, &self.signature, public_key)
    }
}

// WASM exports
#[wasm_bindgen]
pub fn create_post(
    author_fingerprint: &str,
    content: &str,
    tags: Option<Vec<String>>,
    reply_to: Option<String>,
) -> Result<JsValue, JsValue> {
    let post = Post::new(
        author_fingerprint.to_string(),
        content.to_string(),
        tags,
        reply_to,
    );
    
    serde_wasm_bindgen::to_value(&post)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn sign_post(post_json: &str, keypair_json: &str) -> Result<JsValue, JsValue> {
    let post: Post = serde_json::from_str(post_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid post JSON: {}", e)))?;
    
    let keypair: KeyPair = serde_json::from_str(keypair_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid keypair JSON: {}", e)))?;
    
    let signed_post = SignedPost::create(post, &keypair)
        .map_err(|e| JsValue::from_str(&e))?;
    
    serde_wasm_bindgen::to_value(&signed_post)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[wasm_bindgen]
pub fn verify_post(signed_post_json: &str, public_key: &str) -> Result<bool, JsValue> {
    let signed_post: SignedPost = serde_json::from_str(signed_post_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid signed post JSON: {}", e)))?;
    
    signed_post.verify(public_key)
        .map_err(|e| JsValue::from_str(&e))
}