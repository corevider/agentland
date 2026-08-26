#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

port="${AGENTLAND_PORT:-9470}"
token="${AGENTLAND_TOKEN:?set AGENTLAND_TOKEN to the running core's token}"
label="${1:-phone}"

address="$(tailscale ip -4 2>/dev/null | head -1 || true)"
if [ -z "$address" ]; then
    echo "No tailnet address found. Install Tailscale and run 'tailscale up' first —" >&2
    echo "Agentland is not meant to be reachable from the public internet." >&2
    exit 1
fi

device="$(curl -sS -H "x-auth-token: ${token}" -H 'content-type: application/json' \
    -d "{\"label\":\"${label}\"}" "http://127.0.0.1:${port}/devices")"

device_token="$(printf '%s' "$device" | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')"
device_id="$(printf '%s' "$device" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"

echo "Paired ${device_id} (${label}) with approve-only scope."
echo
echo "Restart the core so it accepts the tailnet address:"
echo "  AGENTLAND_HOST=${address} \\"
echo "  AGENTLAND_ALLOWED_HOSTS=\"${address}:${port},127.0.0.1:${port},localhost:${port}\" \\"
echo "  AGENTLAND_MOBILE_DIR=\$PWD/apps/mobile agentland-core"
echo
echo "Then open this on the phone, on the same tailnet:"
echo "  http://${address}:${port}/mobile/?token=${device_token}"
echo
echo "Revoke it any time:"
echo "  curl -X DELETE -H \"x-auth-token: \$AGENTLAND_TOKEN\" http://127.0.0.1:${port}/devices/${device_id}"
