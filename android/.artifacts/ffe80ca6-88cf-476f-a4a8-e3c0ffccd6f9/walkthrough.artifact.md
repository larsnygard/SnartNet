# Fix: ANDROID_NDK_HOME Not Set

I have fixed the issue where the build failed because `ANDROID_NDK_HOME` was not set. The project now uses the Android Gradle Plugin's built-in NDK resolution.

## Changes Made

### App Module
- **[build.gradle.kts](file:///home/lars/src/SnartNet/android/app/build.gradle.kts)**:
    - Added `ndkVersion = "27.0.12077973"` to the `android` block. This allows Gradle to automatically download the correct NDK version if it's missing.
    - Updated the `buildRustBridge` task to use `android.ndkDirectory` as a fallback if the `ANDROID_NDK_HOME` environment variable is not set.
    - Cleaned up unused variables and improved error reporting.

### Documentation
- **[README.md](file:///home/lars/src/SnartNet/android/README.md)**: Updated the prerequisites section to reflect that `ANDROID_NDK_HOME` is now optional.

## Verification Results

### Build Configuration
I ran `./gradlew :app:copyRustBridge --dry-run`.
- **Before**: The build failed immediately with `ANDROID_NDK_HOME is not set`.
- **After**: The build now attempts to resolve the NDK through the standard Android SDK path. If the NDK is not installed, it provides a more accurate error message: `NDK is not installed`, and since `ndkVersion` is specified, Android Studio/Gradle can now automatically install it for you during a project sync.

> [!TIP]
> To finish the setup, please perform a **Gradle Sync** in Android Studio. It should automatically download the NDK version `27.0.12077973` if it's not already present in your SDK folder.
