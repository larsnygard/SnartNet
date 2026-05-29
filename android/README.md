# SnartNet Android Shell

This is a minimal Android host app that loads the shared Rust core through JNI.

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

On Windows PowerShell:

```powershell
rustup target add aarch64-linux-android
./build-android.ps1
```

Set `ANDROID_NDK_HOME` if it is not already configured in your environment.

Output APK (debug):

- `android/app/build/outputs/apk/debug/app-debug.apk`

## Rust bridge crate

- Crate: `android-bridge`
- Library loaded by Android: `snartnet_android_bridge`
- JNI entry class: `com.snartnet.android.NativeBridge`
