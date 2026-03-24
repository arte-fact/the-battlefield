#!/usr/bin/env bash
set -euo pipefail

TARGETS=("aarch64-unknown-linux-gnu" "armv7-unknown-linux-gnueabihf")
TARGET="${1:-aarch64-unknown-linux-gnu}"
PROFILE="${2:-release}"

if [[ ! " ${TARGETS[*]} " =~ " ${TARGET} " ]]; then
    echo "Error: target must be one of: ${TARGETS[*]}"
    exit 1
fi

command -v cross >/dev/null 2>&1 || { echo "Install cross: cargo install cross"; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "Docker is required for cross"; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== Cross-compiling battlefield for ${TARGET} (${PROFILE}) ==="
cross build --target "${TARGET}" --package battlefield-sdl --"${PROFILE}"

# Also build the headless benchmark if available
cross build --target "${TARGET}" --package battlefield-core --"${PROFILE}" --bin bench-headless 2>/dev/null || true

STAGE="target/${TARGET}/${PROFILE}"
echo ""
echo "Binary: ${STAGE}/battlefield"
file "${STAGE}/battlefield" 2>/dev/null || true

case "${TARGET}" in
    aarch64-unknown-linux-gnu)
        echo ""
        echo "To run with QEMU user-mode:"
        echo "  qemu-aarch64 -L /usr/aarch64-linux-gnu ${STAGE}/battlefield"
        ;;
    armv7-unknown-linux-gnueabihf)
        echo ""
        echo "To run with QEMU user-mode:"
        echo "  qemu-arm -L /usr/arm-linux-gnueabihf ${STAGE}/battlefield"
        ;;
esac
