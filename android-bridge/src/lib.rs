//! JNI boundary for the Android host: a frontend of the shared backend service (M10.1).
//!
//! Android talks to the same `snartnet-daemon` service every other frontend uses (ADR 0001).
//! This crate only starts that service inside the app process and forwards requests to it over
//! the daemon's loopback API, so there is exactly one writer of local state no matter how many
//! activities, screens, or observers the app creates, and the UI can be recreated at any time
//! without losing a queued message.
//!
//! The device is treated as a phone (M10.3): `SNARTNET_PLATFORM=mobile` is set before the
//! service opens its state, which selects the mobile storage defaults (no replica hosting, a
//! 64 MiB budget, 7-day leases) instead of the desktop ones.
//!
//! Lifecycle and power (M10.2) become a daemon sync mode. A phone that is on screen stays
//! `Balanced`; a phone that is backgrounded *and* saving battery pauses its scheduler
//! entirely, so a hidden app does no radio work; charging, or hidden without battery saver,
//! stays `Balanced`. The daemon owns its own cadence, so the activity lifecycle no longer
//! decides when the network is used.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use jni::{
    objects::{JClass, JString},
    sys::{jboolean, jstring},
    JNIEnv,
};
use serde_json::{json, Map, Value};
use snartnet_daemon::{run_with, DaemonPaths};
use snartnet_sdk::{Client, Command, Snapshot, SyncMode};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// How long `nativeInit` waits for the in-process service to answer on its API.
const READY_TIMEOUT: Duration = Duration::from_secs(20);

/// Loopback API port. 0 asks the OS for a free one; the daemon publishes where it bound.
const API_PORT: u16 = 0;

/// The peer-facing bind. Android has no operator setting for it, and the auxiliary torrent and
/// DHT ports are derived from it (47473/47474).
const PEER_BIND: &str = "0.0.0.0:47470";

/// The platform value a phone reports, so the storage policy picks the mobile defaults.
const PLATFORM: &str = "mobile";

static SERVICE: OnceLock<Mutex<Option<Service>>> = OnceLock::new();

/// The shared backend service this process talks to.
struct Service {
    client: Client,
    /// The sync mode last applied, so a lifecycle callback that changes nothing costs nothing.
    mode: SyncMode,
}

fn service() -> &'static Mutex<Option<Service>> {
    SERVICE.get_or_init(|| Mutex::new(None))
}

/// Map the app's lifecycle and the device's power state to a sync mode (M10.2).
///
/// The policy is deliberately simple and explainable: on screen means prompt syncing; hidden
/// while saving battery means no syncing at all; hidden and not saving (or charging) keeps the
/// balanced cadence. `AlwaysOn` is never chosen for a phone: nothing in a hidden app needs a
/// sync every few seconds, and the battery would pay for it.
pub fn sync_mode_for(visible: bool, power_save: bool, charging: bool) -> SyncMode {
    if visible {
        return SyncMode::Balanced;
    }
    if power_save && !charging {
        return SyncMode::Paused;
    }
    SyncMode::Balanced
}

/// Flatten a daemon snapshot into the shape the Android UI reads.
///
/// `ClientState` keeps the signed records and the network status as JSON, and the extras are
/// its own keys, so flattening restores the flat object the UI consumed before the backend
/// moved behind the daemon.
fn snapshot_payload(snapshot: &Snapshot) -> Value {
    let state = &snapshot.state;
    let mut payload = Map::new();
    payload.insert(
        "profile".into(),
        state.profile.clone().unwrap_or(Value::Null),
    );
    payload.insert(
        "identityUri".into(),
        state
            .identity_uri
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    payload.insert("posts".into(), Value::Array(state.posts.clone()));
    payload.insert("contacts".into(), Value::Array(state.contacts.clone()));
    payload.insert("threads".into(), Value::Array(state.threads.clone()));
    for (key, value) in &state.extra {
        payload.insert(key.clone(), value.clone());
    }
    Value::Object(payload)
}

fn string(env: &mut JNIEnv, input: JString) -> Result<String, String> {
    env.get_string(&input)
        .map(|s| s.into())
        .map_err(|e| e.to_string())
}

fn reply(env: &mut JNIEnv, result: Result<Value, String>) -> jstring {
    let value = match result {
        Ok(p) => json!({"ok": true, "payload": p}),
        Err(e) => json!({"ok": false, "error": e}),
    };
    env.new_string(value.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Start the shared backend service in this process and wait until it answers.
///
/// The service writes its own runtime metadata, so readiness is judged by asking it: a metadata
/// file that appears before the listener is bound would otherwise be a false start.
fn start_service(root: &str) -> Result<Client, String> {
    // The platform decides the storage defaults (M9.1), and it has to be known before the
    // service opens its state. An explicit value from the embedding host still wins.
    if std::env::var("SNARTNET_PLATFORM").is_err() {
        std::env::set_var("SNARTNET_PLATFORM", PLATFORM);
    }
    migrate_legacy(Path::new(root))?;
    let paths = DaemonPaths::from_data_dir(Some(root))?;
    let thread_paths = paths.clone();
    std::thread::Builder::new()
        .name("snartnet-service".into())
        .spawn(move || {
            if let Err(error) = run_with(thread_paths, API_PORT, PEER_BIND.parse().unwrap()) {
                // A service that cannot serve is reported through the API it never opened, so
                // the only place left to say it is the log the embedding host collects.
                eprintln!("snartnet backend service stopped: {error}");
            }
        })
        .map_err(|e| format!("cannot start the backend service: {e}"))?;
    let client = Client::new(paths).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        match client.health() {
            Ok(_) => return Ok(client),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(format!("the backend service did not start: {error}")),
        }
    }
}

/// The client for the running service, when it is up.
fn client() -> Result<Client, String> {
    service()
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map(|service| service.client.clone())
        .ok_or_else(|| "Client is not initialized".to_string())
}

/// The invitation link, which the QR commands and the share sheet both need.
fn invitation(client: &Client) -> Result<String, String> {
    let response = client
        .command(&Command::Invite)
        .map_err(|e| e.to_string())?;
    response
        .result
        .get("uri")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "the service returned no invitation".to_string())
}

/// Encode the invitation as an image (kept local: it is image work, not state).
fn qr_image(client: &Client, format: &str) -> Result<Value, String> {
    let code = qrcode::QrCode::with_error_correction_level(
        invitation(client)?.as_bytes(),
        qrcode::EcLevel::L,
    )
    .map_err(|e| e.to_string())?;
    if format == "svg" {
        let svg = code
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(640, 640)
            .quiet_zone(true)
            .build();
        return Ok(json!({"data": STANDARD.encode(svg)}));
    }
    let image = image::DynamicImage::ImageLuma8(
        code.render::<image::Luma<u8>>()
            .min_dimensions(640, 640)
            .quiet_zone(true)
            .build(),
    );
    let mut data = std::io::Cursor::new(Vec::new());
    image
        .write_to(
            &mut data,
            if format == "jpg" {
                image::ImageFormat::Jpeg
            } else {
                image::ImageFormat::Png
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({"data": STANDARD.encode(data.into_inner())}))
}

/// Read an invitation out of a picture and hand it to the service as a contact command.
fn import_qr(client: &Client, path: &str) -> Result<Value, String> {
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);
    let mut prepared =
        rqrr::PreparedImage::prepare(reader.decode().map_err(|e| e.to_string())?.to_luma8());
    for grid in prepared.detect_grids() {
        if let Ok((_, content)) = grid.decode() {
            if snartnet_core::ContactInvite::parse(&content).is_ok() {
                return Ok(client
                    .command(&Command::Contact {
                        input: content,
                        mode: String::new(),
                        alias: String::new(),
                        address: String::new(),
                    })
                    .map_err(|e| e.to_string())?
                    .result);
            }
        }
    }
    Err("No valid SnartNet invitation found in this image".into())
}

/// Forward one UI request to the shared service.
fn dispatch(client: &Client, request: &Value) -> Result<Value, String> {
    match request["op"].as_str().unwrap_or("") {
        // Picture work and the initial fetch never change state, so the bridge answers them
        // directly instead of sending a command the service would have to reject.
        "qr" => qr_image(client, request["format"].as_str().unwrap_or("png")),
        "importQr" => import_qr(client, request["path"].as_str().ok_or("Missing image")?),
        "snapshot" => Ok(snapshot_payload(
            &client.snapshot().map_err(|e| e.to_string())?,
        )),
        _ => {
            let command: Command = serde_json::from_value(request.clone())
                .map_err(|e| format!("unsupported command: {e}"))?;
            Ok(client.command(&command).map_err(|e| e.to_string())?.result)
        }
    }
}

/// Preserve identities created by the old SQLite shell; never delete its database.
///
/// The daemon imports whatever it finds in the data directory when it opens its own store, so
/// this runs before the service starts and only copies a legacy identity into the layout the
/// importer understands.
fn migrate_legacy(root: &Path) -> Result<(), String> {
    use snartnet_core::{FileStorage, KeyPair, SignedProfile, SqliteStorage, StorageBackend};
    let data = FileStorage::new(root.join("data")).map_err(|e| e.to_string())?;
    if data
        .get_item("client_state")
        .map_err(|e| e.to_string())?
        .is_some()
        || data
            .get_item("profile")
            .map_err(|e| e.to_string())?
            .is_some()
    {
        return Ok(());
    }
    let old = root.parent().unwrap_or(root).join("snartnet_android.db");
    if !old.exists() {
        return Ok(());
    }
    SqliteStorage::open(old.to_str().ok_or("Invalid storage path")?).map_err(|e| e.to_string())?;
    let kp = SqliteStorage::get_json::<KeyPair>("snartnet_keypair").map_err(|e| e.to_string())?;
    let profile = SqliteStorage::get_json::<SignedProfile>("snartnet_current_profile")
        .map_err(|e| e.to_string())?;
    if let Some(kp) = kp {
        data.set_json("keypair", &kp).map_err(|e| e.to_string())?;
    }
    if let Some(profile) = profile {
        data.set_json("profile", &profile)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeInit(
    mut env: JNIEnv,
    _: JClass,
    root: JString,
) -> jstring {
    let result = (|| {
        let root = string(&mut env, root)?;
        let mut guard = service().lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let client = start_service(&root)?;
            // A phone starts on the balanced cadence, not always-on: the lifecycle decides when
            // it is allowed to work (M10.2).
            let _ = client.set_sync_mode(SyncMode::Balanced);
            *guard = Some(Service {
                client,
                mode: SyncMode::Balanced,
            });
        }
        let client = guard
            .as_ref()
            .ok_or("the backend service did not start")?
            .client
            .clone();
        drop(guard);
        let snapshot = client.snapshot().map_err(|e| e.to_string())?;
        Ok(snapshot_payload(&snapshot))
    })();
    reply(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeCommand(
    mut env: JNIEnv,
    _: JClass,
    request: JString,
) -> jstring {
    let result = (|| {
        let request: Value =
            serde_json::from_str(&string(&mut env, request)?).map_err(|e| e.to_string())?;
        dispatch(&client()?, &request)
    })();
    reply(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeSync(
    mut env: JNIEnv,
    _: JClass,
) -> jstring {
    let result = (|| {
        // One round in the service: publish, ingest, push (M7). Serialising concurrent rounds is
        // the service's job, so a second tap here is harmless.
        let response = client()?.sync().map_err(|e| e.to_string())?;
        Ok(json!({"synced": true, "received": response.received}))
    })();
    reply(&mut env, result)
}

/// Report the app's visibility and the device's power state (M10.2).
///
/// Only a change is sent to the service, so an activity that resumes twice does not talk to the
/// service twice.
#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeSetLifecycle(
    mut env: JNIEnv,
    _: JClass,
    visible: jboolean,
    power_save: jboolean,
    charging: jboolean,
) -> jstring {
    let result = (|| {
        let visible = visible != 0;
        let power_save = power_save != 0;
        let charging = charging != 0;
        let mode = sync_mode_for(visible, power_save, charging);
        let mut guard = service().lock().map_err(|e| e.to_string())?;
        let service = guard.as_mut().ok_or("Client is not initialized")?;
        if service.mode != mode {
            service
                .client
                .set_sync_mode(mode)
                .map_err(|e| e.to_string())?;
            service.mode = mode;
        }
        Ok(json!({
            "mode": mode,
            "visible": visible,
            "powerSave": power_save,
            "charging": charging,
        }))
    })();
    reply(&mut env, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lifecycle policy is the whole of M10.2, so it is pinned by a test rather than by a
    /// comment: on screen synchronizes, hidden-and-saving does not, and nothing gets
    /// `AlwaysOn` on a phone.
    #[test]
    fn lifecycle_and_power_select_a_sync_mode() {
        assert_eq!(sync_mode_for(true, false, false), SyncMode::Balanced);
        assert_eq!(sync_mode_for(true, true, false), SyncMode::Balanced);
        assert_eq!(sync_mode_for(false, false, false), SyncMode::Balanced);
        assert_eq!(sync_mode_for(false, true, false), SyncMode::Paused);
        // A charging phone is not saving battery, so a hidden app may still sync.
        assert_eq!(sync_mode_for(false, true, true), SyncMode::Balanced);
        for (visible, power_save, charging) in [
            (true, true, false),
            (false, false, false),
            (false, true, true),
        ] {
            assert_ne!(
                sync_mode_for(visible, power_save, charging),
                SyncMode::AlwaysOn,
                "a phone never syncs always-on"
            );
        }
    }
}
