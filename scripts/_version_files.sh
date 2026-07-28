#!/usr/bin/env bash
#
# scripts/_version_files.sh — shared library for the 3-file version rewrites.
#
# JADB has THREE places that must agree on a single version string:
#   1. package.json              -> .version
#   2. src-tauri/tauri.conf.json -> .version
#   3. src-tauri/Cargo.toml      -> [package].version
#
# (Note: NiceSSH has a FOURTH — release-please-manifest.json — because
# NiceSSH uses release-please. JADB does not, so this is 3-file only.)
#
# Source this from manage.sh / release.sh / set-version.sh. Do not run it
# directly; sourcing only defines functions.

set -euo pipefail

# ---- readers ----------------------------------------------------------------

read_pkg_version()    { jq -r '.version' package.json; }
read_tauri_version()  { jq -r '.version' src-tauri/tauri.conf.json; }
read_cargo_version() {
  awk '/^\[package\]/{in_pkg=1; next} /^\[/{in_pkg=0} in_pkg && /^version[[:space:]]*=/{print; exit}' \
    src-tauri/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/'
}

# ---- current major.minor -----------------------------------------------------
#
# Reads the first 2 segments from any of the 3 files. They should all agree
# (drift is checked separately), so we just pick the first one.
get_major_minor() {
  local v
  v="$(read_pkg_version)"
  echo "${v%.*}"
}

# ---- drift check -------------------------------------------------------------

check_drift() {
  local pkg tauri cargo
  pkg="$(read_pkg_version)"
  tauri="$(read_tauri_version)"
  cargo="$(read_cargo_version)"

  if [ "$pkg" = "$tauri" ] && [ "$tauri" = "$cargo" ]; then
    return 0
  fi

  echo "::error::Version drift detected across the 3 source files:" >&2
  echo "  package.json:           $pkg" >&2
  echo "  tauri.conf.json:        $tauri" >&2
  echo "  src-tauri/Cargo.toml:   $cargo" >&2
  echo "" >&2
  echo "Fix manually, or use 'manage.sh' option 3 (set-version) to rewrite all 3 at once." >&2
  return 1
}

# ---- commit count ------------------------------------------------------------

commit_count() {
  git rev-list --count HEAD
}

# ---- rewrite all 3 files to a target version --------------------------------

rewrite_3_files() {
  local target="$1"

  # 1. package.json
  local tmp
  tmp="$(mktemp)"
  jq --arg v "$target" '.version = $v' package.json > "$tmp"
  mv "$tmp" package.json

  # 2. src-tauri/tauri.conf.json
  tmp="$(mktemp)"
  jq --arg v "$target" '.version = $v' src-tauri/tauri.conf.json > "$tmp"
  mv "$tmp" src-tauri/tauri.conf.json

  # 3. src-tauri/Cargo.toml (POSIX awk, BSD-sed portable)
  tmp="$(mktemp)"
  awk -v ver="$target" '
    BEGIN { in_pkg = 0; done_ = 0 }
    /^\[package\]/ { in_pkg = 1; print; next }
    /^\[/            { in_pkg = (done_ ? in_pkg : 0) }
    in_pkg && !done_ && /^version[[:space:]]*=/ {
      sub(/^version[[:space:]]*=[[:space:]]*".*"/, "version = \"" ver "\"")
      done_ = 1
    }
    { print }
  ' src-tauri/Cargo.toml > "$tmp"
  mv "$tmp" src-tauri/Cargo.toml
}

# ---- rollback helpers --------------------------------------------------------
#
# These undo a partial rewrite. They are best-effort: if git checkout fails
# (sandbox, lock contention), they fall back to per-file checkout and print
# a warning. The caller is expected to be in a clean-enough state that
# 'git checkout HEAD -- <file>' will restore the original content.

rollback_3_files() {
  if ! git checkout -- package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml 2>/dev/null; then
    echo "::warning::git checkout rollback failed; falling back to per-file HEAD restore." >&2
    for f in package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml; do
      [ -f "$f" ] && git checkout HEAD -- "$f" 2>/dev/null || true
    done
  fi
}

# ---- preflight checks --------------------------------------------------------

require_clean_tree() {
  if [ -n "$(git status --porcelain)" ]; then
    echo "::error::Working tree is not clean. Commit or stash your changes first:" >&2
    git status --short >&2
    return 1
  fi
}

require_jq() {
  if ! command -v jq >/dev/null 2>&1; then
    echo "::error::jq is required but not installed. Install with: brew install jq" >&2
    return 1
  fi
}

require_main_branch() {
  local branch
  branch="$(git rev-parse --abbrev-ref HEAD)"
  if [ "$branch" != "main" ]; then
    echo "::error::Must be on 'main' (currently on '${branch}'). Run: git checkout main" >&2
    return 1
  fi
}
