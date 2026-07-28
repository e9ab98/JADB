#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION=$(jq -r .version package.json)
echo "Building JADB v${VERSION}..."

# Frontend typecheck + tests
pnpm lint
pnpm test

# Rust tests (release mode)
(cd src-tauri && cargo test --release)

# Bundle
pnpm tauri build

DMG=$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -name "*.dmg" | head -1 || true)
if [[ -n "${DMG}" ]]; then
  echo "Done: ${DMG}"
else
  echo "Done (no dmg artifact found, check src-tauri/target/release/bundle/)"
fi
