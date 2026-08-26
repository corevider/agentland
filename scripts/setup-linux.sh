#!/usr/bin/env bash
set -euo pipefail

PACKAGES=(
    libwebkit2gtk-4.1-dev
    libgtk-3-dev
    libayatana-appindicator3-dev
    librsvg2-dev
    libxdo-dev
    libssl-dev
    build-essential
    pkg-config
)

missing=()
for package in "${PACKAGES[@]}"; do
    if ! dpkg -s "$package" >/dev/null 2>&1; then
        missing+=("$package")
    fi
done

if [ ${#missing[@]} -eq 0 ]; then
    echo "Build dependencies are already installed."
else
    echo "Installing: ${missing[*]}"
    sudo apt-get update
    sudo apt-get install -y "${missing[@]}"
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "Installing the Rust toolchain."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

echo "Ready. Next: cargo run -p agentland-core --bin agentland-core"
