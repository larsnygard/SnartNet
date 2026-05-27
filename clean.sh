#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT_DIR"

echo "Cleaning SnartNet native workspace..."
rm -rf target
rm -rf core/pkg core/.wasm-pack

echo "Cleaning legacy web artifacts (optional cache cleanup)..."
rm -rf legacy/PWA/dist legacy/PWA/node_modules legacy/PWA/.vite
rm -rf legacy/PWA/.pnpm-store legacy/PWA/.turbo legacy/PWA/.tsbuildinfo
rm -rf legacy/PWA/wasm/*
rm -f legacy/PWA/pnpm-lock.yaml legacy/PWA/package-lock.json legacy/PWA/yarn.lock

echo "Clean complete."
