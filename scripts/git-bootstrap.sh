#!/usr/bin/env bash
# Bootstrap git in this directory and commit the Task 1 scaffold.
# The Codex session could not init .git here (macOS-level block),
# but a normal terminal on the user's machine usually can.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -d .git ]; then
  echo ".git/ already exists; skipping init."
else
  git init
  git config user.email "codex@local"
  git config user.name "Codex"
fi

git add -A
git commit -m "chore(scaffold): Tauri 2 + React 18 + Tailwind + NiceSSH tokens"
git log --oneline
