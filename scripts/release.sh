#!/usr/bin/env bash
# scripts/release.sh — LOCAL smoke test only.
#
# The canonical release pipeline is `.github/workflows/build-release.yml`:
#   git tag vX.Y.Z && git push origin vX.Y.Z
# tauri-action builds all (matrix) platforms, signs, and uploads the
# release + `latest.json` for the updater.
#
# This script exists for two reasons:
#   1. Quick local QA on the current platform (catches obvious breakage
#      before you push a tag).
#   2. Reproducing what CI does without uploading anything.
#
# It intentionally does NOT upload, sign, or push to GitHub. Run
# `git tag ... && git push --tags` to actually publish.
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION=$(jq -r .version package.json)
echo "Building JADB v${VERSION} (local smoke test, no upload)..."

# Frontend typecheck + tests
pnpm lint
pnpm test

# Rust tests (release mode)
(cd src-tauri && cargo test --release)

# Bundle for the current host only. CI handles cross-platform matrix.
pnpm tauri build

DMG=$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -name "*.dmg" 2>/dev/null | head -1 || true)
MSI=$(find src-tauri/target/release/bundle/msi -maxdepth 1 -name "*.msi" 2>/dev/null | head -1 || true)
if [[ -n "${DMG}" || -n "${MSI}" ]]; then
  echo "Done (local artifacts):"
  [[ -n "${DMG}" ]] && echo "  - ${DMG}"
  [[ -n "${MSI}" ]] && echo "  - ${MSI}"
else
  echo "Done (no installer artifact found under src-tauri/target/release/bundle/)"
fi
