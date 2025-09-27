#!/bin/bash
# Build script for SnartNet PWA (Rust WASM + PWA frontend)
# Usage: ./build.sh
set -e

# 1. Build Rust WASM core
if ! command -v wasm-pack &> /dev/null; then
  echo "wasm-pack not found. Installing..."
  cargo install wasm-pack
fi

cd "$(dirname "$0")"

if [ ! -d "core" ]; then
  echo "Error: core/ directory not found. Run from the SnartNet repo root."
  exit 1
fi

cd core
wasm-pack build --target web --out-dir ../PWA/src/wasm --release
cd ..

# 2. Build PWA frontend
cd PWA
if [ ! -d "node_modules" ]; then
  echo "Installing npm dependencies..."
  npm install
fi

echo "Building PWA..."
npm run build

echo "Build complete! Output in PWA/dist/"
