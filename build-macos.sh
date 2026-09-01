#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

for command in cargo npm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Missing required command: $command" >&2
    exit 1
  fi
done

if ! cargo tauri --version >/dev/null 2>&1; then
  echo "The Tauri CLI is unavailable. Install it with: cargo install tauri-cli --version '^2'" >&2
  exit 1
fi

cargo tauri build

app_path="target/release/bundle/macos/FPV Editor.app"
if [[ ! -d "$app_path" ]]; then
  echo "Build completed but no macOS app bundle was found at: $app_path" >&2
  exit 1
fi

echo
echo "macOS app: $repo_root/$app_path"
find target/release/bundle/dmg -maxdepth 1 -name '*.dmg' -print 2>/dev/null || true
