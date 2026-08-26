#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

target="${1:-$HOME/.agentland/updater.key}"
mkdir -p "$(dirname "$target")"

if [ -f "$target" ]; then
    echo "A key already exists at $target — refusing to overwrite it."
    exit 1
fi

npm --prefix apps/desktop exec -- tauri signer generate -w "$target"

echo
echo "Private key: $target  (never commit this; CI reads it from a secret)"
echo "Put the printed public key into apps/desktop/src-tauri/tauri.conf.json under plugins.updater.pubkey"
echo "Set AGENTLAND_UPDATER_ENDPOINTS at runtime; with no endpoint the updater stays off."
