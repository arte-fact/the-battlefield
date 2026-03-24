#!/usr/bin/env bash
set -euo pipefail

IMAGE_DIR="$(cd "$(dirname "$0")" && pwd)/images"
mkdir -p "${IMAGE_DIR}"

QCOW2="${IMAGE_DIR}/debian-arm64.qcow2"
EFI_VARS="${IMAGE_DIR}/efivars.fd"

# Debian 12 (Bookworm) arm64 cloud image — boots cleanly with QEMU virt + EFI
DEBIAN_URL="https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-arm64.qcow2"

if [[ ! -f "${QCOW2}" ]]; then
    echo "=== Downloading Debian 12 arm64 cloud image ==="
    wget -c -O "${QCOW2}" "${DEBIAN_URL}"
    echo "Resizing image to 8GB..."
    qemu-img resize "${QCOW2}" 8G
    echo "Image ready: ${QCOW2}"
else
    echo "Image already exists: ${QCOW2}"
fi

# EFI firmware vars (writable copy)
EFI_SOURCE="/usr/share/AAVMF/AAVMF_VARS.fd"
if [[ ! -f "${EFI_VARS}" ]]; then
    if [[ -f "${EFI_SOURCE}" ]]; then
        cp "${EFI_SOURCE}" "${EFI_VARS}"
        echo "EFI vars ready: ${EFI_VARS}"
    else
        echo "WARNING: ${EFI_SOURCE} not found."
        echo "Install it: sudo apt-get install qemu-efi-aarch64"
    fi
fi

echo ""
echo "=== Setup complete ==="
echo "Run: ./launch.sh"
