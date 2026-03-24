#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-aarch64-unknown-linux-gnu}"
PROFILE="release"

echo "=== The Battlefield — QEMU ARM (${TARGET}) ==="
echo ""

# --- Prerequisites ---
command -v cross >/dev/null 2>&1 || {
    echo "Error: 'cross' not found. Install it:"
    echo "  cargo install cross --locked"
    exit 1
}
command -v docker >/dev/null 2>&1 || {
    echo "Error: Docker is required for cross-compilation."
    exit 1
}
if ! docker info >/dev/null 2>&1; then
    echo "Error: Docker daemon is not running."
    exit 1
fi

# --- Detect display server and configure container ---
DISPLAY_OPTS=""
SDL_VIDEO=""

if [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
    # Find the Wayland socket
    XDG="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    WSOCK="${XDG}/${WAYLAND_DISPLAY}"
    if [[ -S "${WSOCK}" ]]; then
        echo "Display: Wayland (${WAYLAND_DISPLAY})"
        SDL_VIDEO="wayland"
        DISPLAY_OPTS="-e WAYLAND_DISPLAY=${WAYLAND_DISPLAY}"
        DISPLAY_OPTS+=" -e XDG_RUNTIME_DIR=/tmp/xdg"
        DISPLAY_OPTS+=" -v ${XDG}:/tmp/xdg"
    fi
fi

# X11 / XWayland fallback
if [[ -n "${DISPLAY:-}" ]]; then
    if [[ -z "${SDL_VIDEO}" ]]; then
        echo "Display: X11 (${DISPLAY})"
        SDL_VIDEO="x11"
    fi
    DISPLAY_OPTS+=" -e DISPLAY=${DISPLAY} -v /tmp/.X11-unix:/tmp/.X11-unix"
fi

if [[ -z "${SDL_VIDEO}" ]]; then
    echo "Error: No display server detected (need WAYLAND_DISPLAY or DISPLAY)."
    exit 1
fi

# Force software rendering — ARM binary can't use host x86 GPU drivers under QEMU
RENDER_OPTS="-e LIBGL_ALWAYS_SOFTWARE=1 -e SDL_RENDER_DRIVER=software -e SDL_VIDEODRIVER=${SDL_VIDEO}"

export CROSS_CONTAINER_OPTS="${DISPLAY_OPTS} ${RENDER_OPTS}"

# --- Build ---
echo ""
echo "--- Cross-compiling for ${TARGET} ---"
cross build --target "${TARGET}" --package battlefield-sdl --"${PROFILE}"

# --- Run ---
echo ""
echo "--- Running under QEMU ARM emulation ---"
echo "(Performance is ~10-30x slower than native — this is expected)"
echo ""
cross run --target "${TARGET}" --package battlefield-sdl --"${PROFILE}"
