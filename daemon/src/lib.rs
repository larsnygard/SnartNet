//! Persistent, single-owner backend and authenticated loopback API.
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde::Serialize;
use serde_json::json;
use snartnet_client::session::Session;
use snartnet_sdk::types::{
    ClientState, Command, CommandResponse, ErrorResponse, Health, RuntimeMetadata, Snapshot,
    StateEvent, StopResponse, SyncMode, SyncModeRequest, SyncResponse, API_VERSION,
};
pub use snartnet_sdk::{Client as DaemonClient, DaemonPaths};
use std::{
    convert::Infallible,
    fs::{self, OpenOptions},
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, oneshot, watch};

struct RuntimeLock {
    _file: fs::File,
}
impl RuntimeLock {
    fn acquire(path: PathBuf) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("open daemon lock: {e}"))?;
        file.try_lock()
            .map_err(|_| "another SnartNet daemon owns this data directory".to_string())?;
        // Keep the file: unlinking a locked inode allows a competing owner after restart.
        Ok(Self { _file: file })
    }
}

struct App {
    session: Mutex<Session>,
    token: String,
    mode: Mutex<SyncMode>,
    revision: Mutex<u64>,
    events: broadcast::Sender<StateEvent>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    stopping: watch::Sender<bool>,
}
type Shared = Arc<App>;

/// Production listener for one `SNARTNET_HOME`.
pub const DEFAULT_API_PORT: u16 = 47469;
/// Production peer-facing bind for the torrent/DHT stack.
pub const DEFAULT_PEER_BIND: &str = "127.0.0.1:47470";

pub fn run(paths: DaemonPaths) -> Result<(), String> {
    run_with(paths, DEFAULT_API_PORT, DEFAULT_PEER_BIND.parse().unwrap())
}

/// Explicit listener and peer bind, used by tests that must run side by side.
///
/// `api_port` 0 asks the OS for a free loopback port; the bound address is
/// published in the runtime metadata, so clients still connect. Each distinct
/// `bind` port also moves the derived torrent/DHT auxiliary ports, which keeps
/// concurrent test daemons on separate sockets.
pub fn run_with(
    paths: DaemonPaths,
    api_port: u16,
    bind: std::net::SocketAddr,
) -> Result<(), String> {
    fs::create_dir_all(paths.runtime_dir()).map_err(|e| e.to_string())?;
    let _lock = RuntimeLock::acquire(paths.lock())?;
    let token = load_or_create_token(&paths)?;
    let mut session = Session::open(paths.data_dir(), bind)?;
    session.start();
    let (events_tx, _) = broadcast::channel(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (stopping, _) = watch::channel(false);
    let app = Arc::new(App {
        session: Mutex::new(session),
        token,
        mode: Mutex::new(SyncMode::Balanced),
        revision: Mutex::new(0),
        events: events_tx,
        shutdown: Mutex::new(Some(shutdown_tx)),
        stopping,
    });
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    // Keep the final backend reference on this synchronous stack, even on startup failure.
    runtime.block_on(run_async(paths, app.clone(), api_port, shutdown_rx))
}
async fn run_async(
    paths: DaemonPaths,
    app: Shared,
    api_port: u16,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, api_port))
        .await
        .map_err(|e| format!("bind loopback API: {e}"))?;
    let metadata = RuntimeMetadata {
        api_version: API_VERSION,
        address: listener.local_addr().map_err(|e| e.to_string())?,
        pid: std::process::id(),
        started_at: now(),
    };
    write_private_json(paths.metadata(), &metadata)?;
    let scheduler = tokio::spawn(sync_scheduler(app.clone()));
    let api = router(app.clone());
    let server = axum::serve(listener, api).with_graceful_shutdown(async move {
        tokio::select! { _ = shutdown_rx => {}, _ = tokio::signal::ctrl_c() => {} }
        app.stopping.send_replace(true);
    });
    let result = server.await.map_err(|e| e.to_string());
    scheduler.abort();
    let _ = scheduler.await;
    let _ = fs::remove_file(paths.metadata());
    result
}

fn router(app: Shared) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/snapshot", get(snapshot))
        .route("/v1/command", post(command))
        .route("/v1/sync", post(sync))
        .route("/v1/sync-mode", post(sync_mode))
        .route("/v1/events", get(events))
        .route("/v1/stop", post(stop))
        .with_state(app.clone())
        .layer(axum::middleware::from_fn_with_state(
            app.clone(),
            authenticate,
        ))
}

async fn sync_scheduler(app: Shared) {
    let mut ticks = 0u64;
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let mode = match app.mode.lock() {
            Ok(mode) => *mode,
            Err(_) => return,
        };
        ticks += 1;
        let due = match mode {
            SyncMode::AlwaysOn => true,
            SyncMode::Balanced => ticks.is_multiple_of(6),
            SyncMode::Paused => false,
        };
        if due {
            let app = app.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(mut session) = app.session.lock() {
                    if session.sync_distributed().is_ok() {
                        publish(&app, "sync");
                    }
                }
            })
            .await;
        }
    }
}

async fn authenticate(
    State(app): State<Shared>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if token == Some(app.token.as_str()) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "missing or invalid daemon token".into(),
            }),
        )
            .into_response()
    }
}
async fn health(State(app): State<Shared>) -> Json<Health> {
    Json(state_summary(&app))
}
async fn snapshot(State(app): State<Shared>) -> Result<Json<Snapshot>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let session = app
            .session
            .lock()
            .map_err(|_| ApiError::internal("session lock poisoned"))?;
        let mut state: ClientState = serde_json::from_value(session.snapshot())
            .map_err(|e| ApiError::internal(e.to_string()))?;
        // The scheduler lives in this process, so its mode is added here: a
        // frontend that only reads the session must still see the real mode.
        let mode = *app
            .mode
            .lock()
            .map_err(|_| ApiError::internal("mode lock poisoned"))?;
        state.extra.insert("syncMode".into(), json!(mode));
        Ok(Json(Snapshot {
            api_version: API_VERSION,
            revision: revision(&app),
            state,
        }))
    })
    .await
    .map_err(|_| ApiError::internal("snapshot worker failed"))?
}
async fn command(
    State(app): State<Shared>,
    Json(request): Json<Command>,
) -> Result<Json<CommandResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut session = app
            .session
            .lock()
            .map_err(|_| ApiError::internal("session lock poisoned"))?;
        let request =
            serde_json::to_value(request).map_err(|e| ApiError::bad_request(e.to_string()))?;
        let response = session.command(request).map_err(ApiError::bad_request)?;
        publish(&app, "snapshot");
        Ok(Json(CommandResponse {
            api_version: API_VERSION,
            revision: revision(&app),
            result: response,
        }))
    })
    .await
    .map_err(|_| ApiError::internal("command worker failed"))?
}
async fn sync(State(app): State<Shared>) -> Result<Json<SyncResponse>, ApiError> {
    if matches!(
        *app.mode
            .lock()
            .map_err(|_| ApiError::internal("mode lock poisoned"))?,
        SyncMode::Paused
    ) {
        return Err(ApiError::bad_request("sync is paused"));
    }
    tokio::task::spawn_blocking(move || {
        let mut session = app
            .session
            .lock()
            .map_err(|_| ApiError::internal("session lock poisoned"))?;
        let received = session.sync_distributed().map_err(ApiError::bad_request)?;
        publish(&app, "sync");
        Ok(Json(SyncResponse {
            received,
            revision: revision(&app),
        }))
    })
    .await
    .map_err(|_| ApiError::internal("sync worker failed"))?
}
async fn sync_mode(
    State(app): State<Shared>,
    Json(request): Json<SyncModeRequest>,
) -> Result<Json<Health>, ApiError> {
    *app.mode
        .lock()
        .map_err(|_| ApiError::internal("mode lock poisoned"))? = request.mode;
    // The scheduler owns cadence while the session owns what a paused daemon
    // still does, so the mode has to reach both. Frontends render the session's
    // flag, and it must never disagree with what this process is doing.
    let paused = matches!(request.mode, SyncMode::Paused);
    let worker = app.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = worker
            .session
            .lock()
            .map_err(|_| ApiError::internal("session lock poisoned"))?;
        session
            .command(json!({"op": "pause", "paused": paused}))
            .map_err(ApiError::bad_request)?;
        publish(&worker, "sync-mode");
        Ok::<(), ApiError>(())
    })
    .await
    .map_err(|_| ApiError::internal("sync-mode worker failed"))??;
    Ok(Json(state_summary(&app)))
}
async fn events(
    State(app): State<Shared>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let receiver = app.events.subscribe();
    let initial = futures::stream::once(std::future::ready(Ok(
        axum::response::sse::Event::default().event("state").data(
            json!(StateEvent {
                api_version: API_VERSION,
                revision: revision(&app),
                kind: "reset".into()
            })
            .to_string(),
        ),
    )));

    let stopping = app.stopping.subscribe();
    let stream =
        futures::stream::unfold((receiver, stopping), |(mut receiver, mut stopping)| async {
            if *stopping.borrow() {
                return None;
            }
            let message = tokio::select! {
                _ = stopping.changed() => return None,
                message = receiver.recv() => message,
            };
            match message {
                Ok(value) => Some((
                    Ok(axum::response::sse::Event::default()
                        .event("state")
                        .data(json!(value).to_string())),
                    (receiver, stopping),
                )),
                Err(_) => None,
            }
        });
    use futures::StreamExt;
    Sse::new(initial.chain(stream)).keep_alive(axum::response::sse::KeepAlive::default())
}
async fn stop(State(app): State<Shared>) -> Json<StopResponse> {
    if let Ok(mut tx) = app.shutdown.lock() {
        if let Some(tx) = tx.take() {
            let _ = tx.send(());
        }
    }
    Json(StopResponse { stopping: true })
}
fn state_summary(app: &Shared) -> Health {
    Health {
        api_version: API_VERSION,
        sync_mode: *app.mode.lock().unwrap_or_else(|e| e.into_inner()),
        revision: revision(app),
    }
}
fn revision(app: &Shared) -> u64 {
    *app.revision.lock().unwrap_or_else(|e| e.into_inner())
}
fn publish(app: &Shared, kind: &str) {
    let mut revision = app.revision.lock().unwrap_or_else(|e| e.into_inner());
    *revision += 1;
    let _ = app.events.send(StateEvent {
        api_version: API_VERSION,
        revision: *revision,
        kind: kind.into(),
    });
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}
impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

fn load_or_create_token(paths: &DaemonPaths) -> Result<String, String> {
    if paths.token().exists() {
        return fs::read_to_string(paths.token())
            .map(|s| s.trim().to_owned())
            .map_err(|e| e.to_string());
    }
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    write_private(&paths.token(), token.as_bytes())?;
    Ok(token)
}
fn write_private_json(path: PathBuf, value: &impl Serialize) -> Result<(), String> {
    write_private(
        &path,
        serde_json::to_string(value)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
}
fn write_private(path: &Path, value: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("missing runtime directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    file.write_all(value).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_talks_to_real_routes_with_concurrent_clients_and_live_sse() {
        let root = tempfile::tempdir().unwrap();
        let paths =
            DaemonPaths::from_data_dir(Some(root.path().join("data").to_str().unwrap())).unwrap();
        fs::create_dir_all(paths.runtime_dir()).unwrap();
        let token = load_or_create_token(&paths).unwrap();
        let mut session = Session::open(paths.data_dir(), "127.0.0.1:0".parse().unwrap()).unwrap();
        // This contract test never starts the TCP fallback listener or publishes an identity.
        session.torrent = None;
        let (events, _) = broadcast::channel(64);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (stopping, _) = watch::channel(false);
        let app = Arc::new(App {
            session: Mutex::new(session),
            token,
            mode: Mutex::new(SyncMode::Balanced),
            revision: Mutex::new(0),
            events,
            shutdown: Mutex::new(Some(shutdown_tx)),
            stopping,
        });
        let server_app = app.clone();
        let server_paths = paths.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                write_private_json(
                    server_paths.metadata(),
                    &RuntimeMetadata {
                        api_version: 1,
                        address: listener.local_addr().unwrap(),
                        pid: 1,
                        started_at: 1,
                    },
                )
                .unwrap();
                ready_tx.send(()).unwrap();
                axum::serve(listener, router(server_app.clone()))
                    .with_graceful_shutdown(async move {
                        let _ = shutdown_rx.await;
                        server_app.stopping.send_replace(true);
                    })
                    .await
                    .unwrap();
            });
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let client = DaemonClient::new(paths.clone()).unwrap();
        assert_eq!(client.health().unwrap().api_version, API_VERSION);
        fs::write(paths.token(), "B".repeat(43)).unwrap();
        assert!(matches!(
            client.snapshot(),
            Err(snartnet_sdk::Error::Unauthorized)
        ));
        fs::write(paths.token(), &app.token).unwrap();
        let mut subscription = client.subscribe();
        assert_eq!(subscription.next_snapshot().unwrap().revision, 0);
        // Consume the initial reset; subsequent reads must observe command events.
        assert_eq!(subscription.next_snapshot().unwrap().revision, 0);
        let writers: Vec<_> = (0..4)
            .map(|index| {
                let client = client.clone();
                std::thread::spawn(move || {
                    client
                        .command(&Command::Read {
                            recipient: format!("contact-{index}"),
                        })
                        .unwrap()
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let snapshot = subscription.next_snapshot().unwrap();
        assert_eq!(snapshot.revision, 4);
        assert_eq!(snapshot.state.threads.len(), 4);
        assert_eq!(
            client.set_sync_mode(SyncMode::Paused).unwrap().sync_mode,
            SyncMode::Paused
        );
        assert!(client.sync().is_err());
        // A frontend only reads snapshots, so both the scheduler mode and the
        // session's paused flag must be visible there.
        let paused = client.snapshot().unwrap();
        assert_eq!(
            paused
                .state
                .extra
                .get("paused")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            paused
                .state
                .extra
                .get("syncMode")
                .and_then(|value| value.as_str()),
            Some("paused")
        );
        client.set_sync_mode(SyncMode::Balanced).unwrap();
        let resumed = client.snapshot().unwrap();
        assert_eq!(
            resumed
                .state
                .extra
                .get("paused")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            resumed
                .state
                .extra
                .get("syncMode")
                .and_then(|value| value.as_str()),
            Some("balanced")
        );
        client.stop().unwrap();
        // Keeping the subscription alive must not prevent graceful shutdown.
        server.join().unwrap();
    }

    #[test]
    fn runtime_token_is_stable_and_private() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let paths = DaemonPaths::from_data_dir(Some(data.to_str().unwrap())).unwrap();
        fs::create_dir_all(paths.runtime_dir()).unwrap();
        let first = load_or_create_token(&paths).unwrap();
        assert_eq!(first.len(), 43);
        assert_eq!(first, load_or_create_token(&paths).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(paths.token()).unwrap().permissions().mode() & 0o077,
                0
            );
        }
    }

    #[test]
    fn runtime_lock_rejects_another_owner() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let paths = DaemonPaths::from_data_dir(Some(data.to_str().unwrap())).unwrap();
        fs::create_dir_all(paths.runtime_dir()).unwrap();
        let _lock = RuntimeLock::acquire(paths.lock()).unwrap();
        assert!(RuntimeLock::acquire(paths.lock()).is_err());
        drop(_lock);
        assert!(RuntimeLock::acquire(paths.lock()).is_ok());
    }
}
