#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT_DIR"

echo "Building SnartNet (native-first workspace)..."

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust from https://rustup.rs"
  exit 1
fi

cargo build --workspace

echo "Build complete."
echo "Legacy web client has been archived at legacy/PWA and is no longer part of the default build."
echo "For Android shell/APK build, run ./build-android.sh (or ./build-android.ps1 on Windows)."
