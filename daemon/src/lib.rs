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
use snartnet_client::limits::{Backoff, BACKOFF_BASE, BACKOFF_MAX};
use snartnet_client::session::Session;
use snartnet_sdk::types::{
    ClientState, Command, CommandResponse, ErrorResponse, Health, RuntimeMetadata, Snapshot,
    StateEvent, StopResponse, SyncHealth, SyncMode, SyncModeRequest, SyncResponse, API_VERSION,
};
pub use snartnet_sdk::{Client as DaemonClient, DaemonPaths};
use std::{
    convert::Infallible,
    fs::{self, OpenOptions},
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, oneshot, watch, Notify};

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
    /// When the next round is due and how long to wait after a failed one (M11.1). Shared so
    /// `/v1/health` can answer without interrupting the scheduler.
    scheduler: Mutex<Scheduler>,
    /// Woken when the mode changes, so a frontend never waits out a backoff to be heard.
    wake: Notify,
}
type Shared = Arc<App>;

/// Production listener for one `SNARTNET_HOME`.
pub const DEFAULT_API_PORT: u16 = 47469;
/// Production peer-facing bind for the torrent/DHT stack.
pub const DEFAULT_PEER_BIND: &str = "127.0.0.1:47470";

/// How long between rounds in always-on mode.
pub const ALWAYS_ON_INTERVAL: Duration = BACKOFF_BASE;
/// How long between rounds in balanced mode: a minute is prompt enough for a desktop client and
/// cheap enough to leave running.
pub const BALANCED_INTERVAL: Duration = Duration::from_secs(60);

/// Decides when the next sync round runs, and how patiently a failing one is retried (M11.1).
///
/// Kept apart from the loop that drives it so the policy is testable without a runtime: the
/// timings are the only thing that varies, and they are all `Instant` arithmetic here.
#[derive(Debug)]
struct Scheduler {
    mode: SyncMode,
    backoff: Backoff,
    next_due: Instant,
    last_error: Option<String>,
}

impl Scheduler {
    /// A scheduler for `mode`, with its first round one interval away.
    fn new(mode: SyncMode, now: Instant) -> Self {
        let mut scheduler = Self {
            mode,
            backoff: Backoff::new(BACKOFF_BASE, BACKOFF_MAX),
            next_due: now,
            last_error: None,
        };
        scheduler.next_due = now + scheduler.interval().unwrap_or(BALANCED_INTERVAL);
        scheduler
    }

    /// How often this mode syncs. `None` for paused, which schedules nothing at all.
    fn interval(&self) -> Option<Duration> {
        match self.mode {
            SyncMode::AlwaysOn => Some(ALWAYS_ON_INTERVAL),
            SyncMode::Balanced => Some(BALANCED_INTERVAL),
            SyncMode::Paused => None,
        }
    }

    /// Adopt a mode from a frontend; the next round is scheduled from `now`.
    fn set_mode(&mut self, mode: SyncMode, now: Instant) {
        if self.mode != mode {
            self.mode = mode;
            self.next_due = now + self.interval().unwrap_or(BALANCED_INTERVAL);
        }
    }

    /// Whether a round is due at `now`.
    fn due(&self, now: Instant) -> bool {
        self.interval().is_some() && now >= self.next_due
    }

    /// How long to sleep before looking again.
    ///
    /// The next round's own interval is the floor, and a failing round's backoff is the ceiling:
    /// waiting for the interval is pointless when the retry is deliberately further away, and
    /// retrying before the interval is what the interval exists to prevent.
    fn sleep(&self, now: Instant) -> Duration {
        let idle = match self.interval() {
            Some(_) => self.next_due.saturating_duration_since(now),
            // Paused: nothing is scheduled, so look again after a balanced interval. A mode
            // change wakes the loop anyway, so this is only a fallback.
            None => BALANCED_INTERVAL,
        };
        self.backoff.delay().max(idle)
    }

    /// Score one round and schedule the next one.
    ///
    /// A round that failed, or that had to refuse inbound objects, raises the failures and the
    /// retry delay. A round that completed clears both, so one bad round does not slow a healthy
    /// daemon down for long.
    fn record(&mut self, round: Result<(), String>, overloaded: bool, now: Instant) {
        self.last_error = match round {
            Err(error) => {
                self.backoff.record_failure();
                Some(error)
            }
            Ok(()) if overloaded => {
                self.backoff.record_failure();
                Some("the durable inbound spool is full; inbound objects are being refused".into())
            }
            Ok(()) => {
                self.backoff.record_success();
                None
            }
        };
        self.next_due = now + self.interval().unwrap_or(BALANCED_INTERVAL);
    }

    /// The scheduler as `/v1/health` and the snapshot report it.
    fn health(&self, now: Instant) -> SyncHealth {
        SyncHealth {
            failures: self.backoff.failures(),
            next_sync_ms: self.next_due.saturating_duration_since(now).as_millis() as u64,
            last_error: self.last_error.clone(),
        }
    }
}

/// One app for one `SNARTNET_HOME`, plus the receiver that stops the server.
///
/// Kept in one place so the daemon's own contract tests build an app the same way the real
/// listener does: a field added here cannot be missing in a test.
fn shared(session: Session, token: String) -> (Shared, oneshot::Receiver<()>) {
    let (events, _) = broadcast::channel(64);
    let (shutdown, shutdown_rx) = oneshot::channel();
    let (stopping, _) = watch::channel(false);
    let mode = SyncMode::Balanced;
    let app = Arc::new(App {
        session: Mutex::new(session),
        token,
        mode: Mutex::new(mode),
        revision: Mutex::new(0),
        events,
        shutdown: Mutex::new(Some(shutdown)),
        stopping,
        scheduler: Mutex::new(Scheduler::new(mode, Instant::now())),
        wake: Notify::new(),
    });
    (app, shutdown_rx)
}

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
    let (app, shutdown_rx) = shared(session, token);
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
    let mut stopping = app.stopping.subscribe();
    loop {
        let now = Instant::now();
        let wait = match app.scheduler.lock() {
            Ok(scheduler) => scheduler.sleep(now),
            Err(_) => return,
        };
        // A mode change or a shutdown wakes the loop instead of waiting out a backoff: a phone
        // that comes back to the foreground must not wait ten minutes for a failed round's timer.
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = app.wake.notified() => {}
            _ = stopping.changed() => break,
        }
        if *stopping.borrow() {
            break;
        }
        let now = Instant::now();
        match app.scheduler.lock() {
            Ok(scheduler) => {
                if !scheduler.due(now) {
                    continue;
                }
            }
            Err(_) => return,
        }
        let round_app = app.clone();
        let round = tokio::task::spawn_blocking(move || {
            let mut session = round_app
                .session
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            // One round: publish durable copies, ingest arrivals, then push over both direct
            // paths. The daemon used to only pull, so a desktop message was never handed to a
            // reachable peer (M7.2).
            session.sync_once()?;
            let overloaded = session.overloaded();
            publish(&round_app, "sync");
            Ok::<bool, String>(overloaded)
        })
        .await
        .unwrap_or_else(|_| Err("sync worker failed".into()));
        let (result, overloaded) = match round {
            Ok(overloaded) => (Ok(()), overloaded),
            Err(error) => (Err(error), false),
        };
        match app.scheduler.lock() {
            Ok(mut scheduler) => scheduler.record(result, overloaded, Instant::now()),
            Err(_) => return,
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
        // Backoff state travels with the snapshot too (M11.1), so a frontend can explain a quiet
        // "next sync in" without a second request.
        let sync_health = app
            .scheduler
            .lock()
            .map_err(|_| ApiError::internal("scheduler lock poisoned"))?
            .health(Instant::now());
        state.extra.insert("sync".into(), json!(sync_health));
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
        let received = session.sync_once().map_err(ApiError::bad_request)?;
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
    // The scheduler owns when rounds run, so the mode has to reach it as well, and the loop is
    // woken so a mode change is acted on at once instead of after the current wait (M11.1).
    app.scheduler
        .lock()
        .map_err(|_| ApiError::internal("scheduler lock poisoned"))?
        .set_mode(request.mode, Instant::now());
    app.wake.notify_one();
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
        // The scheduler's retry state (M11.1), so a frontend or a release check can see that the
        // daemon is backing off instead of working normally.
        sync: Some(
            app.scheduler
                .lock()
                .map(|scheduler| scheduler.health(Instant::now()))
                .unwrap_or_else(|e| e.into_inner().health(Instant::now())),
        ),
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
        let session = Session::open(paths.data_dir(), "127.0.0.1:0".parse().unwrap()).unwrap();
        // This contract test never starts the TCP fallback listener or publishes an identity.
        // The ephemeral bind owns no auxiliary socket, so there is no torrent node to unset.
        let (app, shutdown_rx) = shared(session, token);
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
        // A scheduler always reports its retry state, and a healthy daemon is not backing off
        // (M11.1).
        let health = client.health().unwrap();
        let sync = health.sync.expect("the daemon reports its scheduler");
        assert_eq!(sync.failures, 0);
        assert_eq!(sync.last_error, None);
        assert!(sync.next_sync_ms > 0);
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
        let paused_health = client.set_sync_mode(SyncMode::Paused).unwrap();
        assert_eq!(paused_health.sync_mode, SyncMode::Paused);
        // The mode reached the scheduler, not just the session: a paused daemon schedules nothing
        // beyond a balanced interval away (M11.1).
        assert!(
            paused_health
                .sync
                .expect("the daemon reports its scheduler")
                .next_sync_ms
                <= BALANCED_INTERVAL.as_millis() as u64
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
        // The backoff state travels with the snapshot too, so a frontend does not need a second
        // request to explain a quiet daemon (M11.1).
        assert_eq!(
            paused
                .state
                .extra
                .get("sync")
                .and_then(|value| value.get("failures"))
                .and_then(|value| value.as_u64()),
            Some(0)
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
    fn the_scheduler_schedules_each_mode_and_backs_off_after_a_failure() {
        let start = Instant::now();
        // Balanced waits a minute before its first round, always-on its own interval.
        let mut balanced = Scheduler::new(SyncMode::Balanced, start);
        assert!(!balanced.due(start));
        assert!(balanced.due(start + BALANCED_INTERVAL));
        let always_on = Scheduler::new(SyncMode::AlwaysOn, start);
        assert!(!always_on.due(start + Duration::from_secs(5)));
        assert!(always_on.due(start + ALWAYS_ON_INTERVAL));

        // A paused scheduler is never due, and resuming schedules from that moment.
        balanced.set_mode(SyncMode::Paused, start + BALANCED_INTERVAL);
        let later = start + Duration::from_secs(3600);
        assert!(!balanced.due(later));
        assert_eq!(balanced.sleep(later), BALANCED_INTERVAL);
        balanced.set_mode(SyncMode::Balanced, start);
        assert!(!balanced.due(start));
        assert!(balanced.due(start + BALANCED_INTERVAL));

        // A failed round doubles the wait and keeps its reason visible.
        balanced.record(Err("disk is full".into()), false, start);
        assert_eq!(balanced.backoff.delay(), BACKOFF_BASE * 2);
        assert_eq!(balanced.last_error.as_deref(), Some("disk is full"));
        assert_eq!(balanced.health(start).failures, 1);
        assert!(
            balanced.sleep(start) >= BACKOFF_BASE * 2,
            "a backoff must not be shortened by the interval"
        );
        // An overloaded round is scored as a failure with its own reason, because a full spool
        // is the one failure that no retry can fix.
        balanced.record(Ok(()), true, start);
        assert_eq!(balanced.health(start).failures, 2);
        assert!(balanced
            .last_error
            .as_deref()
            .expect("an overloaded round has a reason")
            .contains("spool is full"));
        // A completed round clears both.
        balanced.record(Ok(()), false, start);
        assert_eq!(balanced.health(start).failures, 0);
        assert_eq!(balanced.last_error, None);
        assert_eq!(
            balanced.health(start).next_sync_ms,
            BALANCED_INTERVAL.as_millis() as u64
        );
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
