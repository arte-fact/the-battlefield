#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IMAGE_DIR="${SCRIPT_DIR}/images"
QCOW2="${IMAGE_DIR}/debian-arm64.qcow2"
EFI_CODE="/usr/share/AAVMF/AAVMF_CODE.fd"
EFI_VARS="${IMAGE_DIR}/efivars.fd"

if [[ ! -f "${QCOW2}" ]]; then
    echo "Disk image not found. Run setup-image.sh first."
    exit 1
fi

if [[ ! -f "${EFI_CODE}" ]]; then
    echo "EFI firmware not found: ${EFI_CODE}"
    echo "Install it: sudo apt-get install qemu-efi-aarch64"
    exit 1
fi

# --- Raspberry Pi 4 profile ---
# Cortex-A72 quad-core, 1GB RAM (adjustable)
RAM="${RAM:-1024}"
CPUS="${CPUS:-4}"
SSH_PORT="${SSH_PORT:-2222}"

# Display mode: "sdl" for graphical, "none" for headless
DISPLAY_MODE="${DISPLAY_MODE:-sdl}"

echo "=== QEMU ARM64 VM (Raspberry Pi 4 profile) ==="
echo "    CPU:  Cortex-A72 x${CPUS}"
echo "    RAM:  ${RAM}M"
echo "    SSH:  ssh -p ${SSH_PORT} root@localhost"
echo "    Display: ${DISPLAY_MODE}"
echo ""
echo "    Adjust with: RAM=2048 CPUS=4 SSH_PORT=2222 DISPLAY_MODE=none ./launch.sh"
echo ""

DISPLAY_ARGS=()
if [[ "${DISPLAY_MODE}" == "none" ]]; then
    DISPLAY_ARGS+=(-nographic)
else
    DISPLAY_ARGS+=(-device virtio-gpu-pci -display "${DISPLAY_MODE}")
fi

qemu-system-aarch64 \
    -machine virt \
    -cpu cortex-a72 \
    -smp "${CPUS}" \
    -m "${RAM}" \
    -drive "file=${QCOW2},format=qcow2,if=virtio" \
    -pflash "${EFI_CODE}" \
    -pflash "${EFI_VARS}" \
    -device virtio-net-pci,netdev=net0 \
    -netdev "user,id=net0,hostfwd=tcp::${SSH_PORT}-:22" \
    -device usb-ehci \
    -device usb-kbd \
    -device usb-mouse \
    "${DISPLAY_ARGS[@]}"
