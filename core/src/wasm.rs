use wasm_bindgen::prelude::*;
use crate::storage::browser::BrowserStorage;
use crate::service::{
    CoreService, CreateProfileRequest, UpdateProfileRequest, ProfileEnvelope, CapabilityDescriptor,
};
use crate::storage::StorageError;

fn storage_err(e: StorageError) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// WASM host: thin wrapper around `CoreService<BrowserStorage>`.
#[wasm_bindgen]
pub struct SnartNetCore {
    inner: CoreService<BrowserStorage>,
}

#[wasm_bindgen]
impl SnartNetCore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: CoreService::new(),
        }
    }

    #[wasm_bindgen]
    pub fn init(&mut self) -> Result<(), JsValue> {
        web_sys::console::log_1(&"SnartNet Core WASM initialized".into());
        self.inner.init().map_err(storage_err)
    }

    // ---- Profile management ----

    #[wasm_bindgen]
    pub fn create_profile(
        &mut self,
        username: &str,
        display_name: Option<String>,
        bio: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .create_profile(username, display_name, bio)
            .map_err(storage_err)
    }

    #[wasm_bindgen]
    pub fn get_current_profile(&self) -> Result<JsValue, JsValue> {
        match self.inner.get_profile() {
            Some(env) => serde_wasm_bindgen::to_value(&env.profile)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}"))),
            None => Ok(JsValue::null()),
        }
    }

    #[wasm_bindgen]
    pub fn update_current_profile(
        &mut self,
        display_name: Option<String>,
        bio: Option<String>,
    ) -> Result<(), JsValue> {
        self.inner.update_profile(display_name, bio).map_err(storage_err)
    }

    // ---- Additive JSON-based API (forward-compatible) ----

    #[wasm_bindgen]
    pub fn create_profile_json(&mut self, payload_json: &str) -> Result<JsValue, JsValue> {
        let req: CreateProfileRequest = serde_json::from_str(payload_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid create_profile_json payload: {e}")))?;
        self.inner
            .create_profile(&req.username, req.display_name, req.bio)
            .map_err(storage_err)?;
        match self.inner.get_profile() {
            Some(env) => serde_wasm_bindgen::to_value(&env)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}"))),
            None => Err(JsValue::from_str("Profile creation failed unexpectedly")),
        }
    }

    #[wasm_bindgen]
    pub fn update_profile_json(&mut self, payload_json: &str) -> Result<JsValue, JsValue> {
        let req: UpdateProfileRequest = serde_json::from_str(payload_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid update_profile_json payload: {e}")))?;
        self.inner
            .update_profile(req.display_name, req.bio)
            .map_err(storage_err)?;
        match self.inner.get_profile() {
            Some(env) => serde_wasm_bindgen::to_value(&env)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}"))),
            None => Err(JsValue::from_str("No current profile")),
        }
    }

    #[wasm_bindgen]
    pub fn get_current_profile_json(&self) -> Result<JsValue, JsValue> {
        match self.inner.get_profile() {
            Some(env) => serde_wasm_bindgen::to_value(&env)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}"))),
            None => Ok(JsValue::null()),
        }
    }

    #[wasm_bindgen]
    pub fn get_capabilities(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&CoreService::<BrowserStorage>::capabilities())
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
    }

    // ---- Posts ----

    #[wasm_bindgen]
    pub fn create_post(
        &self,
        content: &str,
        tags: Option<Vec<String>>,
        reply_to: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let signed_post = self.inner.create_post(content, tags, reply_to).map_err(storage_err)?;
        serde_wasm_bindgen::to_value(&signed_post)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
    }

    // ---- Messages ----

    #[wasm_bindgen]
    pub fn create_message(
        &self,
        recipient_fingerprint: &str,
        content: &str,
    ) -> Result<JsValue, JsValue> {
        let signed_msg = self
            .inner
            .create_message(recipient_fingerprint, content)
            .map_err(storage_err)?;
        serde_wasm_bindgen::to_value(&signed_msg)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
    }

    // ---- Utility ----

    #[wasm_bindgen]
    pub fn get_public_key(&self) -> Result<String, JsValue> {
        self.inner
            .get_public_key()
            .map(|s| s.to_string())
            .ok_or_else(|| JsValue::from_str("No keypair available"))
    }

    #[wasm_bindgen]
    pub fn get_fingerprint(&self) -> Result<String, JsValue> {
        self.inner
            .get_fingerprint()
            .map(|s| s.to_string())
            .ok_or_else(|| JsValue::from_str("No keypair available"))
    }

    #[wasm_bindgen]
    pub fn has_profile(&self) -> bool {
        self.inner.has_profile()
    }
}