use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use snartnet_core::{CoreService, SqliteStorage};
use std::sync::{Mutex, OnceLock};

static CORE: OnceLock<Mutex<CoreService<SqliteStorage>>> = OnceLock::new();

fn core() -> &'static Mutex<CoreService<SqliteStorage>> {
    CORE.get_or_init(|| Mutex::new(CoreService::<SqliteStorage>::new()))
}

fn get_string(env: &mut JNIEnv, input: JString) -> Result<String, String> {
    env.get_string(&input)
        .map(|s| s.into())
        .map_err(|e| format!("jni string read failed: {e}"))
}

fn make_jstring(env: &mut JNIEnv, value: &str) -> jstring {
    match env.new_string(value) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn optional_text(v: String) -> Option<String> {
    let t = v.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn ok_json(payload: serde_json::Value) -> String {
    serde_json::json!({
        "ok": true,
        "payload": payload
    })
    .to_string()
}

fn err_json(message: String) -> String {
    serde_json::json!({
        "ok": false,
        "error": message
    })
    .to_string()
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    db_path: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let path = get_string(&mut env, db_path)?;
        SqliteStorage::open(&path).map_err(|e| e.to_string())?;
        let mut svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        svc.init().map_err(|e| e.to_string())?;
        Ok(ok_json(serde_json::json!({ "initialized": true })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeCreateProfile(
    mut env: JNIEnv,
    _class: JClass,
    username: JString,
    display_name: JString,
    bio: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let username = get_string(&mut env, username)?;
        let display_name = optional_text(get_string(&mut env, display_name)?);
        let bio = optional_text(get_string(&mut env, bio)?);

        let mut svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        let magnet_uri = svc
            .create_profile(&username, display_name, bio)
            .map_err(|e| e.to_string())?;
        let profile = svc.get_profile().ok_or("profile missing after creation")?;

        Ok(ok_json(serde_json::json!({
            "magnetUri": magnet_uri,
            "profile": profile
        })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGetProfileJson(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        match svc.get_profile() {
            Some(profile) => Ok(ok_json(serde_json::to_value(profile).map_err(|e| e.to_string())?)),
            None => Ok(ok_json(serde_json::json!({ "profile": null }))),
        }
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeCreatePost(
    mut env: JNIEnv,
    _class: JClass,
    content: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let content = get_string(&mut env, content)?;
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        let post = svc
            .create_post(&content, None, None)
            .map_err(|e| e.to_string())?;
        Ok(ok_json(serde_json::to_value(post).map_err(|e| e.to_string())?))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeCreateMessage(
    mut env: JNIEnv,
    _class: JClass,
    recipient_fingerprint: JString,
    content: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let recipient_fingerprint = get_string(&mut env, recipient_fingerprint)?;
        let content = get_string(&mut env, content)?;

        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        let msg = svc
            .create_message(&recipient_fingerprint, &content)
            .map_err(|e| e.to_string())?;
        Ok(ok_json(serde_json::to_value(msg).map_err(|e| e.to_string())?))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}
