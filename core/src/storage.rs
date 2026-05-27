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
    use std::path::{Path, PathBuf};

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

    /// File-backed storage that persists data in a directory on disk.
    /// Each key maps to a file named `<key>.json` inside the storage directory.
    pub struct FileStorage {
        dir: PathBuf,
    }

    impl FileStorage {
        pub fn new(dir: impl AsRef<Path>) -> Result<Self, StorageError> {
            let dir = dir.as_ref().to_path_buf();
            std::fs::create_dir_all(&dir)
                .map_err(|e| StorageError::Backend(format!("failed to create storage dir: {e}")))?;
            Ok(Self { dir })
        }

        /// Default storage directory: `~/.snartnet/data/`
        pub fn default_dir() -> Result<PathBuf, StorageError> {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map_err(|_| StorageError::Unavailable("Cannot determine home directory".to_string()))?;
            Ok(PathBuf::from(home).join(".snartnet").join("data"))
        }

        /// Open the default storage directory, creating it if necessary.
        pub fn open_default() -> Result<Self, StorageError> {
            Self::new(Self::default_dir()?)
        }

        fn key_path(&self, key: &str) -> PathBuf {
            // Sanitise key so it is safe as a filename.
            let safe_key = key.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            self.dir.join(format!("{safe_key}.json"))
        }

        pub fn set_item(&self, key: &str, value: &str) -> Result<(), StorageError> {
            std::fs::write(self.key_path(key), value)
                .map_err(|e| StorageError::Backend(format!("write failed for {key}: {e}")))
        }

        pub fn get_item(&self, key: &str) -> Result<Option<String>, StorageError> {
            let path = self.key_path(key);
            if !path.exists() {
                return Ok(None);
            }
            let value = std::fs::read_to_string(&path)
                .map_err(|e| StorageError::Backend(format!("read failed for {key}: {e}")))?;
            Ok(Some(value))
        }

        pub fn remove_item(&self, key: &str) -> Result<(), StorageError> {
            let path = self.key_path(key);
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| StorageError::Backend(format!("remove failed for {key}: {e}")))?;
            }
            Ok(())
        }

        pub fn set_json<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<(), StorageError> {
            let json = serde_json::to_string_pretty(value)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            self.set_item(key, &json)
        }

        pub fn get_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>, StorageError> {
            match self.get_item(key)? {
                Some(json) => {
                    let value = serde_json::from_str(&json)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                    Ok(Some(value))
                }
                None => Ok(None),
            }
        }
    }

    pub type LocalStorage = NativeMemoryStorage;
}

#[cfg(target_arch = "wasm32")]
pub use browser::{LocalStorage, storage_get_item, storage_remove_item, storage_set_item};
#[cfg(not(target_arch = "wasm32"))]
pub use native::{FileStorage, LocalStorage};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn native_memory_storage_roundtrip() {
        // NativeMemoryStorage uses a global static — use unique keys per test.
        LocalStorage::set_item("test_mem_key", "hello").expect("set failed");
        let val = LocalStorage::get_item("test_mem_key").expect("get failed");
        assert_eq!(val.as_deref(), Some("hello"));
        LocalStorage::remove_item("test_mem_key").expect("remove failed");
        let val = LocalStorage::get_item("test_mem_key").expect("get after remove failed");
        assert!(val.is_none());
    }

    #[test]
    fn native_memory_storage_json_roundtrip() {
        let map: HashMap<String, u32> = [("a".to_string(), 1u32)].into_iter().collect();
        LocalStorage::set_json("test_mem_json", &map).expect("set_json failed");
        let loaded: Option<HashMap<String, u32>> = LocalStorage::get_json("test_mem_json").expect("get_json failed");
        assert_eq!(loaded.as_ref().and_then(|m| m.get("a")).copied(), Some(1));
        LocalStorage::remove_item("test_mem_json").ok();
    }

    #[test]
    fn file_storage_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir failed");
        let fs = FileStorage::new(dir.path()).expect("FileStorage::new failed");
        fs.set_item("mykey", "myvalue").expect("set failed");
        let v = fs.get_item("mykey").expect("get failed");
        assert_eq!(v.as_deref(), Some("myvalue"));
        fs.remove_item("mykey").expect("remove failed");
        let v2 = fs.get_item("mykey").expect("get after remove failed");
        assert!(v2.is_none());
    }

    #[test]
    fn file_storage_missing_key_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir failed");
        let fs = FileStorage::new(dir.path()).expect("FileStorage::new failed");
        let v = fs.get_item("nonexistent").expect("get failed");
        assert!(v.is_none());
    }

    #[test]
    fn file_storage_json_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir failed");
        let fs = FileStorage::new(dir.path()).expect("FileStorage::new failed");
        let data: Vec<String> = vec!["one".to_string(), "two".to_string()];
        fs.set_json("list", &data).expect("set_json failed");
        let loaded: Option<Vec<String>> = fs.get_json("list").expect("get_json failed");
        assert_eq!(loaded, Some(data));
    }
}