#!/usr/bin/env bash
set -e

echo "🧹 Cleaning SnartNet workspace..."

# Remove frontend build artifacts
echo "🧼 Removing PWA build output and cache..."
rm -rf PWA/dist PWA/node_modules PWA/.vite PWA/.pnpm-store PWA/.turbo PWA/.tsbuildinfo

# Remove Rust WASM output
echo "🧼 Removing WASM bindings..."
rm -rf PWA/wasm/*

# Remove lockfiles and pnpm store
echo "🧼 Removing lockfiles..."
rm -f PWA/pnpm-lock.yaml PWA/package-lock.json PWA/yarn.lock

# Optional: remove Rust target cache (can be large)
echo "🧼 Removing Rust build cache..."
rm -rf target

# Optional: remove core build artifacts
echo "🧼 Removing core WASM artifacts..."
rm -rf core/pkg core/.wasm-pack

echo "✅ Clean complete. You can now run ./build.sh to rebuild everything from scratch."

