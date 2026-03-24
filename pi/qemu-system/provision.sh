#!/usr/bin/env bash
set -euo pipefail

SSH_PORT="${SSH_PORT:-2222}"
SSH_USER="${SSH_USER:-root}"
SSH_CMD="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p ${SSH_PORT} ${SSH_USER}@localhost"

echo "=== Provisioning QEMU VM (${SSH_USER}@localhost:${SSH_PORT}) ==="
echo ""
echo "Make sure the VM is running (./launch.sh) and SSH is accessible."
echo ""

${SSH_CMD} << 'REMOTE'
set -e

echo "--- Installing build dependencies ---"
apt-get update
apt-get install -y \
    curl build-essential cmake git pkg-config \
    libsdl2-dev libx11-dev libasound2-dev \
    libpulse-dev libudev-dev libdbus-1-dev \
    libgl-dev libgles-dev libegl-dev \
    htop

echo ""
echo "--- Installing Rust ---"
if ! command -v rustup &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

rustup update stable
echo "Rust $(rustc --version) installed"

echo ""
echo "--- Provisioning complete ---"
echo "Copy the project into the VM:"
echo "  scp -P 2222 -r /path/to/the-battlefield root@localhost:~/the-battlefield/"
echo "Then build:"
echo "  cd ~/the-battlefield && cargo build --package battlefield-sdl --release"
REMOTE
