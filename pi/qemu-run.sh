#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-aarch64-unknown-linux-gnu}"
PROFILE="${2:-release}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${ROOT}/target/${TARGET}/${PROFILE}/battlefield"

if [[ ! -f "${BINARY}" ]]; then
    echo "Binary not found: ${BINARY}"
    echo "Run: ./pi/cross-build.sh ${TARGET} ${PROFILE}"
    exit 1
fi

case "${TARGET}" in
    aarch64-unknown-linux-gnu)
        QEMU="qemu-aarch64"
        SYSROOT="/usr/aarch64-linux-gnu"
        ;;
    armv7-unknown-linux-gnueabihf)
        QEMU="qemu-arm"
        SYSROOT="/usr/arm-linux-gnueabihf"
        ;;
    *)
        echo "Unknown target: ${TARGET}"
        exit 1
        ;;
esac

command -v "${QEMU}" >/dev/null 2>&1 || { echo "Install QEMU: sudo apt-get install qemu-user-static"; exit 1; }

echo "=== Running ${BINARY} under ${QEMU} ==="

# SDL2 needs a display; use xvfb for headless environments
if [[ -z "${DISPLAY:-}" ]] && command -v xvfb-run >/dev/null 2>&1; then
    echo "(No DISPLAY set, using xvfb-run)"
    cd "$ROOT"
    RUST_LOG="${RUST_LOG:-info}" xvfb-run "${QEMU}" -L "${SYSROOT}" "${BINARY}"
else
    cd "$ROOT"
    RUST_LOG="${RUST_LOG:-info}" "${QEMU}" -L "${SYSROOT}" "${BINARY}"
fi
