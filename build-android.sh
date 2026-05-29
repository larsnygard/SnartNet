#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT_DIR"

echo "Building SnartNet Android shell..."

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust from https://rustup.rs"
  exit 1
fi

if ! command -v ./android/gradlew >/dev/null 2>&1 && ! command -v gradle >/dev/null 2>&1; then
  echo "Gradle not found. Install Gradle or add Android Gradle Wrapper files."
  exit 1
fi

rustup target add aarch64-linux-android >/dev/null 2>&1 || true

if [ -x ./android/gradlew ]; then
  (cd android && ./gradlew :app:assembleDebug)
else
  (cd android && gradle :app:assembleDebug)
fi

echo "Android build complete: android/app/build/outputs/apk/debug/app-debug.apk"
