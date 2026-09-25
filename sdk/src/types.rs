//! Version 1 local API. Unknown response fields are accepted for additive evolution.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;

pub const API_VERSION: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetadata {
    pub api_version: u64,
    pub address: SocketAddr,
    pub pid: u32,
    pub started_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncMode {
    AlwaysOn,
    /// The mode every daemon starts in, and the assumption for a missing value.
    #[default]
    Balanced,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub api_version: u64,
    pub revision: u64,
    pub sync_mode: SyncMode,
}

/// Full authoritative state; profile and network records retain their existing JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub api_version: u64,
    pub revision: u64,
    pub state: ClientState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientState {
    pub profile: Option<Value>,
    pub identity_uri: Option<String>,
    #[serde(default)]
    pub posts: Vec<Value>,
    #[serde(default)]
    pub contacts: Vec<Value>,
    #[serde(default)]
    pub threads: Vec<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Profile {
        username: String,
        #[serde(rename = "displayName", default)]
        display_name: String,
        #[serde(default)]
        bio: String,
        #[serde(default)]
        avatar: String,
        #[serde(default)]
        address: String,
    },
    Contact {
        input: String,
        #[serde(default)]
        mode: String,
        #[serde(default)]
        alias: String,
        #[serde(default)]
        address: String,
    },
    Post {
        content: String,
    },
    Message {
        recipient: String,
        content: String,
    },
    Read {
        recipient: String,
    },
    Discovery {
        enabled: bool,
    },
    Cleanup,
    Invite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    pub api_version: u64,
    pub revision: u64,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEvent {
    pub api_version: u64,
    pub revision: u64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncModeRequest {
    pub mode: SyncMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub received: usize,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopResponse {
    pub stopping: bool,
}
