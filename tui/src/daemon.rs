//! Daemon-backed terminal client (ADR 0001).
//!
//! Every snapshot and every action goes through `snartnet-sdk`. The terminal UI
//! owns no identity, SQLite database, torrent session, or peer listener; the
//! daemon stays authoritative and this process only renders and requests.

use serde_json::Value;
use snartnet_sdk::{Client, Command, DaemonPaths, Error, Snapshot, Subscription, SyncMode};
use std::path::PathBuf;

/// Named like the daemon binary in `cli/Cargo.toml`.
const DAEMON_BINARY: &str = if cfg!(windows) {
    "snartnet.exe"
} else {
    "snartnet"
};

#[derive(Clone)]
pub(crate) struct Backend {
    client: Client,
}

/// Values only the daemon can produce, because only it holds the identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Invite {
    pub uri: String,
    pub magnet: Option<String>,
}

impl Backend {
    /// Uses the same `SNARTNET_DATA_DIR` / `SNARTNET_HOME` resolution as the CLI.
    pub(crate) fn connect(data_dir: Option<&str>) -> Result<Self, String> {
        Self::connect_to(DaemonPaths::from_data_dir(data_dir)?)
    }

    /// Explicit daemon location, so tests can talk to a daemon they started.
    pub(crate) fn connect_to(paths: DaemonPaths) -> Result<Self, String> {
        Ok(Self {
            client: Client::new(paths).map_err(describe)?,
        })
    }

    /// The installed daemon beside this frontend, if the packager shipped both.
    pub(crate) fn daemon_executable() -> Option<PathBuf> {
        let sibling = std::env::current_exe().ok()?.parent()?.join(DAEMON_BINARY);
        sibling.is_file().then_some(sibling)
    }

    /// Starts the daemon only when it is missing or unreachable.
    pub(crate) fn ensure_running(&self) -> Result<(), String> {
        let executable = Self::daemon_executable().ok_or_else(|| {
            format!("Place the `{DAEMON_BINARY}` daemon beside this program, then retry.")
        })?;
        self.client
            .ensure_running(&executable)
            .map(|_| ())
            .map_err(describe)
    }

    pub(crate) fn snapshot(&self) -> Result<Snapshot, String> {
        self.client.snapshot().map_err(describe)
    }

    /// Daemon-pushed invalidation, so the view follows daemon changes without polling.
    pub(crate) fn subscribe(&self) -> Subscription {
        self.client.subscribe()
    }

    pub(crate) fn command(&self, command: Command) -> Result<(), String> {
        self.client.command(&command).map(|_| ()).map_err(describe)
    }

    pub(crate) fn sync(&self) -> Result<usize, String> {
        self.client
            .sync()
            .map(|response| response.received)
            .map_err(describe)
    }

    pub(crate) fn set_sync_mode(&self, mode: SyncMode) -> Result<SyncMode, String> {
        self.client
            .set_sync_mode(mode)
            .map(|health| health.sync_mode)
            .map_err(describe)
    }

    /// Graceful daemon shutdown. Quitting this view never does this implicitly.
    pub(crate) fn stop(&self) -> Result<(), String> {
        self.client.stop().map(|_| ()).map_err(describe)
    }

    /// Invitation links stay daemon-generated so the key never leaves the daemon.
    pub(crate) fn invite(&self) -> Result<Invite, String> {
        let response = self.client.command(&Command::Invite).map_err(describe)?;
        invite_from_result(&response.result)
    }
}

pub(crate) fn invite_from_result(result: &Value) -> Result<Invite, String> {
    let uri = result
        .get("uri")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "daemon returned no invitation link".to_string())?;
    Ok(Invite {
        uri,
        magnet: result
            .get("magnet")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

/// Terminal failures (authentication, version, protocol) must stay visible and
/// must never trigger an automatic daemon start.
pub(crate) fn describe(error: Error) -> String {
    match error {
        Error::Unavailable => "The SnartNet daemon is not running for this data directory.".into(),
        Error::Transport => {
            "The SnartNet daemon stopped responding. Restart it, then refresh.".into()
        }
        Error::Unauthorized | Error::InvalidRuntime => {
            "The daemon rejected this program's runtime credentials. Restart the daemon.".into()
        }
        other => other.to_string(),
    }
}
