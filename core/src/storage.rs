use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
pub enum StorageError {
    Unavailable(String),
    Serialization(String),
    Backend(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Unavailable(msg) => write!(f, "Storage unavailable: {msg}"),
            StorageError::Serialization(msg) => write!(f, "Serialization error: {msg}"),
            StorageError::Backend(msg) => write!(f, "Storage backend error: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}

pub trait StorageBackend {
    fn set_item(key: &str, value: &str) -> Result<(), StorageError>;
    fn get_item(key: &str) -> Result<Option<String>, StorageError>;
    fn remove_item(key: &str) -> Result<(), StorageError>;

    fn set_json<T: Serialize>(key: &str, value: &T) -> Result<(), StorageError> {
        let json = serde_json::to_string(value)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        Self::set_item(key, &json)
    }

    fn get_json<T: DeserializeOwned>(key: &str) -> Result<Option<T>, StorageError> {
        match Self::get_item(key)? {
            Some(json) => {
                let value = serde_json::from_str(&json)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{StorageBackend, StorageError};
    use wasm_bindgen::prelude::*;
    use web_sys::{Storage, Window};

    pub struct BrowserStorage;

    impl BrowserStorage {
        fn get_storage() -> Result<Storage, StorageError> {
            let window: Window = web_sys::window()
                .ok_or_else(|| StorageError::Unavailable("No global window object".to_string()))?;
            window
                .local_storage()
                .map_err(|e| StorageError::Backend(format!("localStorage access failed: {e:?}")))?
                .ok_or_else(|| StorageError::Unavailable("No localStorage available".to_string()))
        }
    }

    impl StorageBackend for BrowserStorage {
        fn set_item(key: &str, value: &str) -> Result<(), StorageError> {
            let storage = Self::get_storage()?;
            storage
                .set_item(key, value)
                .map_err(|e| StorageError::Backend(format!("set_item failed: {e:?}")))
        }

        fn get_item(key: &str) -> Result<Option<String>, StorageError> {
            let storage = Self::get_storage()?;
            storage
                .get_item(key)
                .map_err(|e| StorageError::Backend(format!("get_item failed: {e:?}")))
        }

        fn remove_item(key: &str) -> Result<(), StorageError> {
            let storage = Self::get_storage()?;
            storage
                .remove_item(key)
                .map_err(|e| StorageError::Backend(format!("remove_item failed: {e:?}")))
        }
    }

    pub struct LocalStorage;

    impl LocalStorage {
        pub fn set_item(key: &str, value: &str) -> Result<(), JsValue> {
            BrowserStorage::set_item(key, value).map_err(|e| JsValue::from_str(&e.to_string()))
        }

        pub fn get_item(key: &str) -> Result<Option<String>, JsValue> {
            BrowserStorage::get_item(key).map_err(|e| JsValue::from_str(&e.to_string()))
        }

        pub fn remove_item(key: &str) -> Result<(), JsValue> {
            BrowserStorage::remove_item(key).map_err(|e| JsValue::from_str(&e.to_string()))
        }

        pub fn set_json<T: serde::Serialize>(key: &str, value: &T) -> Result<(), JsValue> {
            BrowserStorage::set_json(key, value).map_err(|e| JsValue::from_str(&e.to_string()))
        }

        pub fn get_json<T: serde::de::DeserializeOwned>(key: &str) -> Result<Option<T>, JsValue> {
            BrowserStorage::get_json(key).map_err(|e| JsValue::from_str(&e.to_string()))
        }
    }

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
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{HashMap, Mutex, OnceLock, StorageBackend, StorageError};

    fn store() -> &'static Mutex<HashMap<String, String>> {
        static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub struct NativeMemoryStorage;

    impl StorageBackend for NativeMemoryStorage {
        fn set_item(key: &str, value: &str) -> Result<(), StorageError> {
            let mut guard = store()
                .lock()
                .map_err(|e| StorageError::Backend(format!("lock poisoned: {e}")))?;
            guard.insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn get_item(key: &str) -> Result<Option<String>, StorageError> {
            let guard = store()
                .lock()
                .map_err(|e| StorageError::Backend(format!("lock poisoned: {e}")))?;
            Ok(guard.get(key).cloned())
        }

        fn remove_item(key: &str) -> Result<(), StorageError> {
            let mut guard = store()
                .lock()
                .map_err(|e| StorageError::Backend(format!("lock poisoned: {e}")))?;
            guard.remove(key);
            Ok(())
        }
    }

    pub type LocalStorage = NativeMemoryStorage;
}

#[cfg(target_arch = "wasm32")]
pub use browser::{LocalStorage, storage_get_item, storage_remove_item, storage_set_item};
#[cfg(not(target_arch = "wasm32"))]
pub use native::LocalStorage;