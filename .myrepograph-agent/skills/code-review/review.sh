#!/usr/bin/env bash
# Mechanical pre-review checks. Fails loudly; the agent reviews what is left.
set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo .)"

echo "== changed files =="
git diff --name-only HEAD 2>/dev/null || echo "(not a git repository)"

if [ -f package.json ]; then
  echo "== frontend build =="
  npm run build
fi

if [ -f src-tauri/Cargo.toml ]; then
  echo "== backend tests =="
  (cd src-tauri && cargo test)
fi

echo "== mechanical checks passed; review the logic by hand =="
