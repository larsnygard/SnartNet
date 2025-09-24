use wasm_bindgen::prelude::*;
use web_sys::{Storage, Window};
use serde_json;

// Simple storage wrapper for browser localStorage
pub struct LocalStorage;

impl LocalStorage {
    fn get_storage() -> Result<Storage, JsValue> {
        let window: Window = web_sys::window().ok_or("No global window object")?;
        window.local_storage()?.ok_or("No localStorage available".into())
    }
    
    pub fn set_item(key: &str, value: &str) -> Result<(), JsValue> {
        let storage = Self::get_storage()?;
        storage.set_item(key, value)
    }
    
    pub fn get_item(key: &str) -> Result<Option<String>, JsValue> {
        let storage = Self::get_storage()?;
        storage.get_item(key)
    }
    
    pub fn remove_item(key: &str) -> Result<(), JsValue> {
        let storage = Self::get_storage()?;
        storage.remove_item(key)
    }
    
    pub fn set_json<T: serde::Serialize>(key: &str, value: &T) -> Result<(), JsValue> {
        let json = serde_json::to_string(value)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
        Self::set_item(key, &json)
    }
    
    pub fn get_json<T: serde::de::DeserializeOwned>(key: &str) -> Result<Option<T>, JsValue> {
        match Self::get_item(key)? {
            Some(json) => {
                let value = serde_json::from_str(&json)
                    .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

// WASM exports
#[wasm_bindgen]
pub fn storage_set_item(key: &str, value: &str) -> Result<(), JsValue> {
    LocalStorage::set_item(key, value)
}

#[wasm_bindgen]
pub fn storage_get_item(key: &str) -> Result<JsValue, JsValue> {
    match LocalStorage::get_item(key)? {
        Some(value) => Ok(JsValue::from_str(&value)),
        None => Ok(JsValue::null()),
    }
}

#[wasm_bindgen]
pub fn storage_remove_item(key: &str) -> Result<(), JsValue> {
    LocalStorage::remove_item(key)
}