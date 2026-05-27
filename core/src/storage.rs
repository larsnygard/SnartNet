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
pub mod browser {
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

    // ---- In-memory backend (used by tests and as a fallback) ----

    use std::cell::RefCell;

    thread_local! {
        static STORE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    }

    pub struct NativeMemoryStorage;

    impl StorageBackend for NativeMemoryStorage {
        fn set_item(key: &str, value: &str) -> Result<(), StorageError> {
            STORE.with(|s| s.borrow_mut().insert(key.to_string(), value.to_string()));
            Ok(())
        }

        fn get_item(key: &str) -> Result<Option<String>, StorageError> {
            Ok(STORE.with(|s| s.borrow().get(key).cloned()))
        }

        fn remove_item(key: &str) -> Result<(), StorageError> {
            STORE.with(|s| s.borrow_mut().remove(key));
            Ok(())
        }
    }

    pub type LocalStorage = NativeMemoryStorage;

    // ---- SQLite backend ----

    use rusqlite::{Connection, params};

    /// Returns the configured database path (default: `"snartnet.db"`).
    /// Call `SqliteStorage::open` before the first storage operation to override it.
    fn configured_path() -> &'static Mutex<String> {
        static PATH: OnceLock<Mutex<String>> = OnceLock::new();
        PATH.get_or_init(|| Mutex::new("snartnet.db".to_string()))
    }

    fn db() -> &'static Mutex<Connection> {
        static DB: OnceLock<Mutex<Connection>> = OnceLock::new();
        DB.get_or_init(|| {
            let path = configured_path()
                .lock()
                .expect("path lock poisoned")
                .clone();
            let conn = Connection::open(&path)
                .expect("failed to open SQLite database");
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS kv_store (
                     key   TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );",
            )
            .expect("failed to create kv_store table");
            Mutex::new(conn)
        })
    }

    pub struct SqliteStorage;

    impl SqliteStorage {
        /// Configure the database path and initialise the schema.
        ///
        /// **Must be called before any `StorageBackend` methods** – once the
        /// first storage operation runs, the path is locked in and subsequent
        /// `open` calls have no effect.
        pub fn open(path: &str) -> Result<(), StorageError> {
            // Store the desired path so that `db()` picks it up on first use.
            {
                let mut guard = configured_path()
                    .lock()
                    .map_err(|e| StorageError::Backend(format!("path lock poisoned: {e}")))?;
                *guard = path.to_string();
            }
            // Force initialisation now, using the path we just set.
            let _ = db();
            Ok(())
        }
    }

    impl StorageBackend for SqliteStorage {
        fn set_item(key: &str, value: &str) -> Result<(), StorageError> {
            let guard = db()
                .lock()
                .map_err(|e| StorageError::Backend(format!("db lock poisoned: {e}")))?;
            guard
                .execute(
                    "INSERT INTO kv_store (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )
                .map_err(|e| StorageError::Backend(format!("SQLite set_item failed: {e}")))?;
            Ok(())
        }

        fn get_item(key: &str) -> Result<Option<String>, StorageError> {
            let guard = db()
                .lock()
                .map_err(|e| StorageError::Backend(format!("db lock poisoned: {e}")))?;
            let mut stmt = guard
                .prepare("SELECT value FROM kv_store WHERE key = ?1")
                .map_err(|e| StorageError::Backend(format!("SQLite prepare failed: {e}")))?;
            let mut rows = stmt
                .query(params![key])
                .map_err(|e| StorageError::Backend(format!("SQLite query failed: {e}")))?;
            match rows
                .next()
                .map_err(|e| StorageError::Backend(format!("SQLite row failed: {e}")))?
            {
                Some(row) => {
                    let val: String = row
                        .get(0)
                        .map_err(|e| StorageError::Backend(format!("SQLite get col failed: {e}")))?;
                    Ok(Some(val))
                }
                None => Ok(None),
            }
        }

        fn remove_item(key: &str) -> Result<(), StorageError> {
            let guard = db()
                .lock()
                .map_err(|e| StorageError::Backend(format!("db lock poisoned: {e}")))?;
            guard
                .execute("DELETE FROM kv_store WHERE key = ?1", params![key])
                .map_err(|e| StorageError::Backend(format!("SQLite remove_item failed: {e}")))?;
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::{LocalStorage, storage_get_item, storage_remove_item, storage_set_item};
#[cfg(not(target_arch = "wasm32"))]
pub use native::{LocalStorage, SqliteStorage};

/// Public alias so that `service.rs` tests can name a concrete backend without
/// caring about the platform.
#[cfg(not(target_arch = "wasm32"))]
pub type MemoryStorage = native::NativeMemoryStorage;