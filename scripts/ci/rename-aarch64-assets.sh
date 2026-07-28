#!/usr/bin/env bash
# scripts/ci/rename-aarch64-assets.sh
# Post-process a Tauri release to rename aarch64-named assets to arm64.
#
# Tauri delegates to the Apple platform toolchain which produces
# `aarch64` in filenames (`JADB_aarch64.app.tar.gz`,
# `JADB_X.Y.Z_aarch64.dmg`); the Windows MSI bundler in contrast uses
# `arm64`. We normalize to `arm64` for naming consistency.
#
# Idempotent: skips cleanly if the aarch64 assets are already gone.
#
# Usage (locally):
#   gh auth login
#   ./scripts/ci/rename-aarch64-assets.sh v0.1.18
#
# Inside CI the env vars GH_TOKEN / GITHUB_REPOSITORY / GITHUB_REF_NAME
# are auto-set so no args are required.
set -euo pipefail

REPO="${REPO:-${GITHUB_REPOSITORY:-e9ab98/JADB}}"
TAG="${1:-${GITHUB_REF_NAME:-}}"

if [ -z "$TAG" ]; then
  echo "usage: $0 <tag>    e.g. $0 v0.1.18"
  exit 1
fi

# Pick the gh CLI flags based on whether we're in CI or local.
if [ -n "${GH_TOKEN:-}" ]; then
  ARGS=(--repo "$REPO")
else
  ARGS=(--repo "$REPO")
fi

echo "Renaming aarch64 \xe2\x86\x92 arm64 in $REPO @ $TAG"

WORK=$(mktemp -d)
cd "$WORK"

# Find current aarch64-named assets in this release.
ASSETS=$(gh release view "$TAG" "${ARGS[@]}" --json assets --jq '.assets[].name' | grep aarch64 || true)
if [ -z "$ASSETS" ]; then
  echo "No aarch64 assets in $TAG; nothing to rename."
  cd /
  rm -rf "$WORK"
  exit 0
fi

# Download everything we plan to rename.
echo "$ASSETS" > assets-to-rename.txt
gh release download "$TAG" "${ARGS[@]}" --pattern '*aarch64*' --dir .

while IFS= read -r f; do
  [ -z "$f" ] && continue
  if [ ! -f "$f" ]; then
    echo "  WARN: $f listed but not downloaded; skipping"
    continue
  fi
  new="${f//aarch64/arm64}"
  echo "  $f \xe2\x86\x92 $new"
  gh release delete-asset "$TAG" "$f" "${ARGS[@]}" --yes
  gh release upload "$TAG" "${ARGS[@]}" "${f}#${new}" --clobber
done < assets-to-rename.txt

# Update latest.json: keep the platform keys intact (they're baked
# into the Rust updater client at compile time) and only swap the URL
# strings that reference the renamed files. We use `walk` so any
# `url` field anywhere in the document gets the substitution.
if gh release download "$TAG" "${ARGS[@]}" --pattern 'latest.json' --dir . 2>/dev/null && [ -f latest.json ]; then
  cp latest.json latest.json.bak
  if jq -e 'walk(if type == "object" and has("url") then .url |= gsub("aarch64"; "arm64") else . end)' \
       latest.json.bak > latest.json 2>/dev/null; then
    if ! cmp -s latest.json latest.json.bak; then
      gh release delete-asset "$TAG" "${ARGS[@]}" 'latest.json' --yes
      gh release upload "$TAG" "${ARGS[@]}" 'latest.json#latest.json' --clobber
      echo "  latest.json: aarch64 \xe2\x86\x92 arm64 in URL strings"
    else
      mv latest.json.bak latest.json
      echo "  latest.json: no aarch64 in URLs; skipping"
    fi
  else
    mv latest.json.bak latest.json
    echo "  latest.json: jq rewrite failed; skipping"
  fi
else
  echo "  latest.json: not present in release; skipping"
fi

cd /
rm -rf "$WORK"
echo "Done."
