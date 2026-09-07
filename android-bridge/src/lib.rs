//! JNI boundary. Network exchanges run without holding the state mutex.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use jni::{
    objects::{JClass, JString},
    sys::jstring,
    JNIEnv,
};
use serde_json::{json, Value};
use snartnet_client::session::Session;
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
static SYNC: Mutex<()> = Mutex::new(());
fn session() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
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
#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeInit(
    mut env: JNIEnv,
    _: JClass,
    root: JString,
) -> jstring {
    let result = (|| {
        let root = string(&mut env, root)?;
        let mut guard = session().lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            migrate_legacy(Path::new(&root))?;
            let mut client = Session::open(Path::new(&root), "0.0.0.0:47470".parse().unwrap())?;
            client.start();
            *guard = Some(client);
        }
        Ok(guard.as_ref().unwrap().snapshot())
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
        let mut guard = session().lock().map_err(|e| e.to_string())?;
        let client = guard.as_mut().ok_or("Client is not initialized")?;
        match request["op"].as_str().unwrap_or("") {
            "qr" => {
                let code = qrcode::QrCode::with_error_correction_level(
                    client.invitation()?.as_bytes(),
                    qrcode::EcLevel::L,
                )
                .map_err(|e| e.to_string())?;
                let format = request["format"].as_str().unwrap_or("png");
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
            "importQr" => {
                let path = request["path"].as_str().ok_or("Missing image")?;
                let mut reader = image::ImageReader::open(path)
                    .map_err(|e| e.to_string())?
                    .with_guessed_format()
                    .map_err(|e| e.to_string())?;
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                reader.limits(limits);
                let mut prepared = rqrr::PreparedImage::prepare(
                    reader.decode().map_err(|e| e.to_string())?.to_luma8(),
                );
                for grid in prepared.detect_grids() {
                    if let Ok((_, content)) = grid.decode() {
                        if snartnet_core::ContactInvite::parse(&content).is_ok() {
                            return client.command(json!({"op":"contact", "input": content}));
                        }
                    }
                }
                Err("No valid SnartNet invitation found in this image".into())
            }
            _ => client.command(request),
        }
    })();
    reply(&mut env, result)
}
#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeSync(
    mut env: JNIEnv,
    _: JClass,
) -> jstring {
    let result = (|| {
        let _sync = SYNC.try_lock().map_err(|_| "Sync already running")?;
        if let Some(client) = session().lock().map_err(|e| e.to_string())?.as_mut() {
            let _ = client.sync_distributed();
        }
        let work = session()
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .ok_or("Client is not initialized")?
            .prepare_sync();
        if let Some(work) = work {
            let result = work();
            session()
                .lock()
                .map_err(|e| e.to_string())?
                .as_mut()
                .ok_or("Client is not initialized")?
                .apply_sync(result)?;
        }
        Ok(json!({"synced":true}))
    })();
    reply(&mut env, result)
}

/// Preserve identities created by the old SQLite shell; never delete its database.
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
