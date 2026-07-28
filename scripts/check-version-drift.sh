#!/usr/bin/env bash
#
# scripts/check-version-drift.sh — CI-friendly wrapper around scripts/_version_files.sh.
#
# Exits 0 if package.json / src-tauri/tauri.conf.json / src-tauri/Cargo.toml
# all have the same .version; exits 1 with diagnostics otherwise.
#
# Wired into:
#   - .github/workflows/build-release.yml  (early fail before publish)
#   - .github/workflows/test.yml           (guard on PR + main + tag pushes)
#
# For interactive management (set-version, tag/release, etc.) use ./manage.sh
# at the repo root, which sources the same _version_files.sh library.
set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck source=scripts/_version_files.sh
. "${PWD}/scripts/_version_files.sh"

require_jq
check_drift
