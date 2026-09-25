//! Blocking frontend SDK. Use from a UI worker thread, never an async runtime worker.
pub mod types;
use reqwest::{
    blocking::{Client as HttpClient, Response},
    Method, StatusCode,
};
use serde::de::DeserializeOwned;
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::Duration,
};
pub use types::*;

#[derive(Debug)]
pub enum Error {
    Unavailable,
    Unauthorized,
    Incompatible { found: u64 },
    InvalidRuntime,
    Protocol(String),
    Http { status: u16, message: String },
    Transport,
    Start(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => {
                write!(f, "daemon is not running or runtime files are unavailable")
            }
            Self::Unauthorized => write!(f, "daemon authentication failed"),
            Self::Incompatible { found } => {
                write!(f, "unsupported daemon API {found}; expected {API_VERSION}")
            }
            Self::InvalidRuntime => write!(f, "invalid daemon runtime metadata or token"),
            Self::Protocol(message) => write!(f, "invalid daemon response: {message}"),
            Self::Http { status, message } => write!(f, "daemon returned HTTP {status}: {message}"),
            Self::Transport => write!(
                f,
                "daemon connection failed; a submitted command may have committed"
            ),
            Self::Start(message) => write!(f, "cannot start daemon: {message}"),
        }
    }
}
impl std::error::Error for Error {}
fn compatible(version: u64) -> Result<(), Error> {
    if version == API_VERSION {
        Ok(())
    } else {
        Err(Error::Incompatible { found: version })
    }
}

#[derive(Clone)]
pub struct DaemonPaths {
    data: PathBuf,
    runtime: PathBuf,
}
impl DaemonPaths {
    pub fn from_data_dir(data_dir: Option<&str>) -> Result<Self, String> {
        let configured = data_dir
            .map(str::to_owned)
            .or_else(|| std::env::var("SNARTNET_DATA_DIR").ok());
        let data = match configured {
            Some(data) => PathBuf::from(data),
            None => {
                let home = std::env::var_os("SNARTNET_HOME")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("HOME")
                            .or_else(|| std::env::var_os("USERPROFILE"))
                            .map(|p| PathBuf::from(p).join(".snartnet"))
                    })
                    .ok_or("cannot determine SnartNet home")?;
                home.join("data")
            }
        };
        let data = if data.is_absolute() {
            data
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(data)
        };
        let runtime = data.parent().unwrap_or(&data).join("runtime");
        Ok(Self { data, runtime })
    }
    pub fn data_dir(&self) -> &Path {
        &self.data
    }
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime
    }
    pub fn metadata(&self) -> PathBuf {
        self.runtime.join("daemon.json")
    }
    pub fn token(&self) -> PathBuf {
        self.runtime.join("api.token")
    }
    pub fn lock(&self) -> PathBuf {
        self.runtime.join("daemon.lock")
    }
}

#[derive(Clone)]
pub struct Client {
    paths: DaemonPaths,
    http: HttpClient,
}
impl Client {
    pub fn new(paths: DaemonPaths) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Self { paths, http })
    }
    fn endpoint(&self) -> Result<(String, String), Error> {
        let metadata: RuntimeMetadata = serde_json::from_slice(
            &fs::read(self.paths.metadata()).map_err(|_| Error::Unavailable)?,
        )
        .map_err(|_| Error::InvalidRuntime)?;
        compatible(metadata.api_version)?;
        if !metadata.address.ip().is_loopback() || metadata.address.port() == 0 {
            return Err(Error::InvalidRuntime);
        }
        let token = fs::read_to_string(self.paths.token()).map_err(|_| Error::Unavailable)?;
        let token = token.trim();
        if token.len() != 43
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(Error::InvalidRuntime);
        }
        Ok((format!("http://{}", metadata.address), token.to_owned()))
    }
    fn send(
        &self,
        method: Method,
        route: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<Response, Error> {
        let (endpoint, token) = self.endpoint()?;
        let mut request = self
            .http
            .request(method, format!("{endpoint}/v1/{route}"))
            .bearer_auth(token);
        if route == "health" {
            request = request.timeout(Duration::from_secs(2));
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().map_err(|_| Error::Transport)?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(Error::Unauthorized),
            status if !status.is_success() => {
                let mut bytes = Vec::new();
                let _ = response.take(65536).read_to_end(&mut bytes);
                let message = serde_json::from_slice::<ErrorResponse>(&bytes)
                    .map(|error| error.error)
                    .unwrap_or_else(|_| "request rejected".into());
                Err(Error::Http {
                    status: status.as_u16(),
                    message,
                })
            }
            _ => Ok(response),
        }
    }
    fn read<T: DeserializeOwned>(&self, route: &str) -> Result<T, Error> {
        for attempt in 0..3 {
            match self.send(Method::GET, route, None).and_then(decode) {
                Err(Error::Transport | Error::Unavailable) if attempt < 2 => {
                    std::thread::sleep(Duration::from_millis(100 << attempt))
                }
                result => return result,
            }
        }
        unreachable!()
    }
    fn write<T: DeserializeOwned>(&self, route: &str, body: serde_json::Value) -> Result<T, Error> {
        self.health()?;
        // Never replay writes: a disconnected response does not mean the command failed.
        decode(self.send(Method::POST, route, Some(&body))?)
    }
    pub fn health(&self) -> Result<Health, Error> {
        let value: Health = self.read("health")?;
        compatible(value.api_version)?;
        Ok(value)
    }
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        let value: Snapshot = self.read("snapshot")?;
        compatible(value.api_version)?;
        Ok(value)
    }
    pub fn command(&self, command: &Command) -> Result<CommandResponse, Error> {
        let value: CommandResponse = self.write(
            "command",
            serde_json::to_value(command).map_err(|e| Error::Protocol(e.to_string()))?,
        )?;
        compatible(value.api_version)?;
        Ok(value)
    }
    pub fn sync(&self) -> Result<SyncResponse, Error> {
        self.write("sync", serde_json::json!({}))
    }
    pub fn set_sync_mode(&self, mode: SyncMode) -> Result<Health, Error> {
        let value: Health = self.write("sync-mode", serde_json::json!({"mode":mode}))?;
        compatible(value.api_version)?;
        Ok(value)
    }
    pub fn stop(&self) -> Result<StopResponse, Error> {
        self.write("stop", serde_json::json!({}))
    }

    /// Starts only for missing/unreachable daemons. Authentication/version failures are terminal.
    /// Pass the installed `snartnet` executable explicitly; frontends can locate a sibling binary.
    pub fn ensure_running(&self, executable: &Path) -> Result<Health, Error> {
        match self.health() {
            Ok(health) => return Ok(health),
            Err(Error::Unavailable | Error::Transport) => {}
            Err(error) => return Err(error),
        }
        let mut child = ProcessCommand::new(executable)
            .arg("--data-dir")
            .arg(self.paths.data_dir())
            .args(["daemon", "run"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::Start(e.to_string()))?;
        // Reap the child eventually, including a racing start that loses the daemon lock.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            match self.health() {
                Ok(health) => return Ok(health),
                Err(Error::Unavailable | Error::Transport) => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(error) => return Err(error),
            }
        }
        Err(Error::Start(
            "readiness deadline exceeded; run `snartnet daemon run` for diagnostics".into(),
        ))
    }
    pub fn subscribe(&self) -> Subscription {
        Subscription {
            client: self.clone(),
            reader: None,
        }
    }
}
fn decode<T: DeserializeOwned>(response: Response) -> Result<T, Error> {
    let mut bytes = Vec::new();
    response
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Transport)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Error::Protocol("response too large".into()));
    }
    serde_json::from_slice(&bytes).map_err(|e| Error::Protocol(e.to_string()))
}

/// Each connect/reconnect opens SSE before fetching a full snapshot, avoiding missed changes.
/// Events invalidate snapshots; no event replay history or monotonic revision across restarts is assumed.
pub struct Subscription {
    client: Client,
    reader: Option<BufReader<Response>>,
}
impl Subscription {
    pub fn next_snapshot(&mut self) -> Result<Snapshot, Error> {
        for attempt in 0..3 {
            match self.next_connected() {
                Err(Error::Transport | Error::Unavailable) if attempt < 2 => {
                    self.reader = None;
                    std::thread::sleep(Duration::from_millis(100 << attempt));
                }
                result => return result,
            }
        }
        unreachable!()
    }
    fn next_connected(&mut self) -> Result<Snapshot, Error> {
        if self.reader.is_none() {
            self.client.health()?;
            let response = self.client.send(Method::GET, "events", None)?;
            if !response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("text/event-stream"))
            {
                return Err(Error::Protocol("expected SSE".into()));
            }
            self.reader = Some(BufReader::new(response));
            return self.client.snapshot();
        }
        let reader = self.reader.as_mut().unwrap();
        let mut data = String::new();
        loop {
            let mut line = String::new();
            let size = (&mut *reader)
                .take(65537)
                .read_line(&mut line)
                .map_err(|_| Error::Transport)?;
            if size == 0 {
                return Err(Error::Transport);
            }
            if size > 65536 || data.len() + size > 65536 {
                return Err(Error::Protocol("event too large".into()));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() && !data.is_empty() {
                let event: StateEvent =
                    serde_json::from_str(&data).map_err(|e| Error::Protocol(e.to_string()))?;
                compatible(event.api_version)?;
                return self.client.snapshot();
            }
            if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.trim_start());
                data.push('\n');
            }
        }
    }
}
