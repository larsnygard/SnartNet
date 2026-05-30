use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use qrcode::render::unicode;
use qrcode::{EcLevel, QrCode};
use serde::{Deserialize, Serialize};
use snartnet_core::{ContactInvite, CoreService, SqliteStorage};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LAN_DISCOVERY_PORT: u16 = 47471;
const BROADCAST_INTERVAL_SECS: u64 = 30;
const SHUTDOWN_CHECK_INTERVAL_MS: u64 = 500;
const PEER_EXPIRY_SECS: u64 = 120;

static CORE: OnceLock<Mutex<CoreService<SqliteStorage>>> = OnceLock::new();
static LAN_DISCOVERY: OnceLock<LanDiscovery> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanAnnounce {
    fingerprint: String,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp_addr: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DiscoveredPeer {
    fingerprint: String,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp_addr: Option<String>,
    last_seen: u64,
}

struct LanDiscovery {
    peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
    active: Arc<AtomicBool>,
}

impl LanDiscovery {
    fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start(&self, announce: LanAnnounce) -> bool {
        if self.active.load(Ordering::Relaxed) {
            return true;
        }

        let listener = match UdpSocket::bind(format!("0.0.0.0:{LAN_DISCOVERY_PORT}")) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let _ = listener.set_read_timeout(Some(Duration::from_millis(500)));

        self.active.store(true, Ordering::Relaxed);
        let active_listener = self.active.clone();
        let active_sender = self.active.clone();
        let peers_listener = self.peers.clone();
        let own_fp = announce.fingerprint.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while active_listener.load(Ordering::Relaxed) {
                if let Ok((len, _)) = listener.recv_from(&mut buf) {
                    let Ok(msg) = serde_json::from_slice::<LanAnnounce>(&buf[..len]) else {
                        continue;
                    };
                    if msg.fingerprint == own_fp {
                        continue;
                    }
                    let now = unix_secs();
                    let mut guard = peers_listener.lock().unwrap();
                    if let Some(existing) =
                        guard.iter_mut().find(|p| p.fingerprint == msg.fingerprint)
                    {
                        existing.last_seen = now;
                        existing.username.clone_from(&msg.username);
                        existing.display_name.clone_from(&msg.display_name);
                        existing.tcp_addr.clone_from(&msg.tcp_addr);
                    } else {
                        guard.push(DiscoveredPeer {
                            fingerprint: msg.fingerprint,
                            username: msg.username,
                            display_name: msg.display_name,
                            tcp_addr: msg.tcp_addr,
                            last_seen: now,
                        });
                    }
                    guard.retain(|p| is_peer_fresh(p.last_seen));
                }
            }
        });

        std::thread::spawn(move || {
            let sender = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(_) => return,
            };
            let _ = sender.set_broadcast(true);
            let broadcast_addr: SocketAddr = format!("255.255.255.255:{LAN_DISCOVERY_PORT}")
                .parse()
                .unwrap();
            while active_sender.load(Ordering::Relaxed) {
                if let Ok(payload) = serde_json::to_vec(&announce) {
                    let _ = sender.send_to(&payload, broadcast_addr);
                }
                let checks = BROADCAST_INTERVAL_SECS * 1000 / SHUTDOWN_CHECK_INTERVAL_MS;
                for _ in 0..checks {
                    if !active_sender.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(SHUTDOWN_CHECK_INTERVAL_MS));
                }
            }
        });

        true
    }

    fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        self.peers.lock().unwrap().clear();
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    fn get_discovered(&self) -> Vec<DiscoveredPeer> {
        let mut guard = self.peers.lock().unwrap();
        guard.retain(|p| is_peer_fresh(p.last_seen));
        guard.clone()
    }
}

fn core() -> &'static Mutex<CoreService<SqliteStorage>> {
    CORE.get_or_init(|| Mutex::new(CoreService::<SqliteStorage>::new()))
}

fn lan_discovery() -> &'static LanDiscovery {
    LAN_DISCOVERY.get_or_init(LanDiscovery::new)
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

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_peer_fresh(last_seen: u64) -> bool {
    unix_secs().saturating_sub(last_seen) < PEER_EXPIRY_SECS
}

fn local_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

fn tcp_sync_hint() -> Option<String> {
    local_lan_ip().map(|ip| format!("{}:{}", ip, LAN_DISCOVERY_PORT - 1))
}

fn generate_qr_text(data: &str) -> String {
    let code = match QrCode::with_error_correction_level(data.as_bytes(), EcLevel::L) {
        Ok(c) => c,
        Err(_) => return "[QR generation failed – data too large]".to_string(),
    };
    code.render::<unicode::Dense1x2>().quiet_zone(true).build()
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
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeUpdateProfile(
    mut env: JNIEnv,
    _class: JClass,
    display_name: JString,
    bio: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let display_name = optional_text(get_string(&mut env, display_name)?);
        let bio = optional_text(get_string(&mut env, bio)?);

        let mut svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        svc.update_profile(display_name, bio)
            .map_err(|e| e.to_string())?;
        let profile = svc.get_profile().ok_or("profile missing after update")?;

        Ok(ok_json(serde_json::json!({ "profile": profile })))
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
            Some(profile) => Ok(ok_json(
                serde_json::to_value(profile).map_err(|e| e.to_string())?,
            )),
            None => Ok(ok_json(serde_json::json!({ "profile": null }))),
        }
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGetPublicKey(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        Ok(ok_json(
            serde_json::json!({ "publicKey": svc.get_public_key() }),
        ))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGetFingerprint(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        Ok(ok_json(
            serde_json::json!({ "fingerprint": svc.get_fingerprint() }),
        ))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGetCapabilities(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        Ok(ok_json(
            serde_json::to_value(CoreService::<SqliteStorage>::capabilities())
                .map_err(|e| e.to_string())?,
        ))
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
        Ok(ok_json(
            serde_json::to_value(post).map_err(|e| e.to_string())?,
        ))
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
        Ok(ok_json(
            serde_json::to_value(msg).map_err(|e| e.to_string())?,
        ))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeExportInviteCode(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        let signed_profile = svc
            .get_signed_profile()
            .ok_or("no profile available for invite")?;
        let invite = ContactInvite::from_signed_profile(signed_profile, tcp_sync_hint());
        let code = invite.to_base64()?;
        Ok(ok_json(serde_json::json!({ "inviteCode": code })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeImportInviteCode(
    mut env: JNIEnv,
    _class: JClass,
    invite_code: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let invite_code = get_string(&mut env, invite_code)?;
        let invite = ContactInvite::from_base64(&invite_code)?;
        Ok(ok_json(
            serde_json::to_value(invite).map_err(|e| e.to_string())?,
        ))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGenerateInviteQr(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        let signed_profile = svc
            .get_signed_profile()
            .ok_or("no profile available for invite")?;
        let invite = ContactInvite::from_signed_profile(signed_profile, tcp_sync_hint());
        let code = invite.to_base64()?;
        let qr = generate_qr_text(&format!("snartnet://invite/{code}"));
        Ok(ok_json(serde_json::json!({
            "inviteCode": code,
            "qrText": qr
        })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeStartLanDiscovery(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let svc = core().lock().map_err(|e| format!("lock failed: {e}"))?;
        let signed_profile = svc
            .get_signed_profile()
            .ok_or("create a profile before starting LAN discovery")?;

        let announce = LanAnnounce {
            fingerprint: signed_profile.profile.fingerprint.clone(),
            username: signed_profile.profile.username.clone(),
            display_name: signed_profile.profile.display_name.clone(),
            tcp_addr: tcp_sync_hint(),
        };

        let started = lan_discovery().start(announce);
        Ok(ok_json(serde_json::json!({ "active": started })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeStopLanDiscovery(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        lan_discovery().stop();
        Ok(ok_json(serde_json::json!({ "active": false })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGetLanDiscoveryStatus(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let peers = lan_discovery().get_discovered();
        Ok(ok_json(serde_json::json!({
            "active": lan_discovery().is_active(),
            "discoveredPeerCount": peers.len()
        })))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}

#[no_mangle]
pub extern "system" fn Java_com_snartnet_android_NativeBridge_nativeGetDiscoveredPeers(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let peers = lan_discovery().get_discovered();
        Ok(ok_json(
            serde_json::to_value(peers).map_err(|e| e.to_string())?,
        ))
    })();

    make_jstring(&mut env, &result.unwrap_or_else(err_json))
}
