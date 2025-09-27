#!/usr/bin/env bash
set -e

echo "📁 Switching to PWA directory..."
cd "$(dirname "$0")/PWA"

echo "🔍 Checking for Node.js..."
if ! which node > /dev/null 2>&1; then
  echo "❌ Node.js not found. Please install it."
  exit 1
fi
echo "✅ Node.js found: $(node -v)"

echo " Checking for rustup..."
if ! command -v rustup > /dev/null; then
  echo "❌ Rustup not found. Please install it from https://rustup.rs for WASM support."
  echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  echo "source \"$HOME/.cargo/env"
  echo "rustup target add wasm32-unknown-unknown"
  exit 1
fi


echo "📦 Installing pnpm locally..."
npm install -g pnpm --prefix "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"

echo "✅ pnpm version: $(pnpm -v)"

echo "📦 Installing dependencies..."
pnpm install

echo "🧹 Cleaning Vite cache (if exists)..."
[ -d node_modules/.vite ] && rm -rf node_modules/.vite

echo "🦀 Building Rust WASM core..."
cd ../core
wasm-pack build --target web --out-dir ../PWA/wasm --out-name snartnet_core
cd ../PWA

echo "🚀 Building the PWA..."
pnpm run build

echo "🎉 Build complete!"

