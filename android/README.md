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

## Backend service, lifecycle, and power (M10)

The Kotlin app is a frontend of the shared backend service (ADR 0001). `android-bridge`
starts `snartnet-daemon` inside the app process, waits for it to answer on its loopback
API, and forwards every UI request to it; the UI no longer holds a session of its own.
That is what makes the app's lifecycle irrelevant to correctness: an activity can be
recreated, rotated, or backgrounded while a queued message, an inbound spool, or a signed
receipt is mid-flight, because the service owns both the write path and the file.

- `nativeInit(root)`: sets `SNARTNET_PLATFORM=mobile` (unless the host set it), imports a
  legacy identity if one exists, starts the service on an OS-assigned loopback port with
  the peer bind `0.0.0.0:47470` (which also derives the torrent and DHT ports, 47473 and
  47474), waits up to 20 seconds for `GET /v1/health`, and returns the flattened snapshot
  the UI already consumed.
- `nativeCommand(request)`: forwards to `POST /v1/command` after parsing the request into
  the SDK's typed command. `qr`, `importQr`, and `snapshot` are answered in the bridge
  because they are image work or a read, not state changes; `importQr` still goes through
  the service as a contact command once a valid invite is decoded.
- `nativeSync()`: one publish/ingest/push round in the service.
- `nativeSetLifecycle(visible, powerSave, charging)`: the lifecycle policy. On screen is
  `Balanced`; hidden while saving battery (and not charging) is `Paused`, so a background
  phone does no radio work at all; hidden and charging, or hidden without battery saver,
  stays `Balanced`. `AlwaysOn` is never selected on a phone. Only a change is sent, and
  `SnartNetService` (a `dataSync` foreground service, started when the app leaves the
  screen) keeps the process alive so the policy is what decides work, not the activity.
- Mobile storage defaults follow from `SNARTNET_PLATFORM=mobile` (M9.1): the app reports
  `storage.platform = "mobile"`, does not host replicas, keeps a 64 MiB budget, and issues
  7-day leases. The Network screen shows that, alongside the delivery, relay, and storage
  blocks.
- Recovery: a process that is killed anyway loses nothing that was committed. The service
  imports its store on the next start, the outbox retries on the next sync, and the inbound
  spool is drained then too (M7.3). The daemon suite already covers a paused service
  refusing sync and a resumed one resuming without a restart, and the bridge has a unit
  test for the lifecycle policy itself.
- Limitation, stated plainly: the service runs in the app process, so Android may still
  stop it when the system reclaims memory (the foreground service notification is what
  asks it not to). A future release can run the daemon as a separate process; nothing in
  the API depends on it being in-process.

## Rust bridge crate

- Crate: `android-bridge`
- Shared client crate: `client`
- Library loaded by Android: `snartnet_android_bridge`
- JNI entry class: `com.snartnet.android.NativeBridge`

The JNI API is intentionally small: `nativeInit`, `nativeCommand`, and `nativeSync`.
Commands are JSON requests so the Android UI and desktop share the same business rules.
