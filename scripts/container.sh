#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -f .env ]; then
    token="$(head -c 32 /dev/urandom | base64 | tr -d '=+/' | cut -c1-32)"
    cat > .env <<ENV
AGENTLAND_TOKEN=${token}
HOST_PORT=9470
AGENTLAND_PROJECTS_DIR=./projects
ENV
    echo "Generated .env with a fresh token."
fi

mkdir -p data/container projects

set -a
# shellcheck disable=SC1091
source .env
set +a

docker compose up -d --build

echo
echo "Agentland core is running in a container."
echo "Open: http://127.0.0.1:${HOST_PORT:-9470}/?token=${AGENTLAND_TOKEN}"
echo "Only ${AGENTLAND_PROJECTS_DIR:-./projects} is visible to it."
