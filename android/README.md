# SnartNet Android client

The Android client now uses the same signed identity, invitation, contact, encrypted
message, post, TCP sync, LAN discovery, QR, and durable outbox behavior as the desktop
client. Rust owns validation, persistence, encryption, and network exchange; the Kotlin
activity is the Android UI and lifecycle adapter.

The app includes Messages, Feed, Contacts, My profile, and Network screens. Messages are
stored as ciphertext and are decrypted only for the active UI view. Queued envelopes retry
after restart. Android may stop the process while it is backgrounded, so sync resumes when
the app returns to the foreground.

## Prerequisites

- Android SDK + platform tools
- Android NDK (r26+ recommended)
- JDK 17
- Rust + Android target

## Quick build

From repository root:

```bash
rustup target add aarch64-linux-android
./build-android.sh
```

For an x86_64 emulator, build both phone and emulator libraries with:

```bash
cd android
./gradlew :app:assembleDebug -PsnartnetAbis=arm64-v8a,x86_64
```

On Windows PowerShell:

```powershell
rustup target add aarch64-linux-android
./build-android.ps1
```

The build script will automatically attempt to find the NDK using the Android Gradle Plugin. You can also explicitly set `ANDROID_NDK_HOME` if you want to use a specific NDK installation.

If your environment cannot resolve `dl.google.com`, configure a Google Maven mirror:

- env var: `SNARTNET_GOOGLE_MIRROR_URLS=https://maven.aliyun.com/repository/google`
- or Gradle property: `-PsnartnetGoogleMirrorUrls=https://maven.aliyun.com/repository/google`

Multiple mirrors are supported as a comma-separated list.

Output APK (debug):

- `android/app/build/outputs/apk/debug/app-debug.apk`

## Rust bridge crate

- Crate: `android-bridge`
- Shared client crate: `client`
- Library loaded by Android: `snartnet_android_bridge`
- JNI entry class: `com.snartnet.android.NativeBridge`

The JNI API is intentionally small: `nativeInit`, `nativeCommand`, and `nativeSync`.
Commands are JSON requests so the Android UI and desktop share the same business rules.
