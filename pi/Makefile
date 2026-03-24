.PHONY: help build-arm64 build-armv7 run-arm64 bench bench-arm64 \
       qemu-setup qemu-launch qemu-provision

SHELL := /bin/bash
ROOT := $(shell cd .. && pwd)
TARGET_ARM64 := aarch64-unknown-linux-gnu
TARGET_ARMV7 := armv7-unknown-linux-gnueabihf

help:
	@echo "=== Raspberry Pi / ARM Testing ==="
	@echo ""
	@echo "Cross-compilation:"
	@echo "  make build-arm64    Cross-compile for Pi 3/4/5 (aarch64)"
	@echo "  make build-armv7    Cross-compile for Pi Zero/2W (armv7)"
	@echo "  make run-arm64      Run ARM64 binary under QEMU user-mode"
	@echo ""
	@echo "Benchmarks:"
	@echo "  make bench          Run Criterion benchmarks (native)"
	@echo "  make bench-arm64    Run headless benchmark (ARM64 via QEMU)"
	@echo ""
	@echo "QEMU System Emulation:"
	@echo "  make qemu-setup     Download and prepare Debian arm64 image"
	@echo "  make qemu-launch    Boot the QEMU ARM64 VM"
	@echo "  make qemu-provision Install build deps in the VM"
	@echo ""
	@echo "Prerequisites:"
	@echo "  cargo install cross"
	@echo "  sudo apt-get install qemu-user-static qemu-system-arm qemu-efi-aarch64"

# --- Cross-compilation ---

build-arm64:
	cd $(ROOT) && cross build --target $(TARGET_ARM64) --package battlefield-sdl --release

build-armv7:
	cd $(ROOT) && cross build --target $(TARGET_ARMV7) --package battlefield-sdl --release

run-arm64: build-arm64
	./qemu-run.sh $(TARGET_ARM64) release

# --- Benchmarks ---

bench:
	cd $(ROOT) && cargo bench --package battlefield-core

bench-arm64:
	cd $(ROOT) && cross build --target $(TARGET_ARM64) --package battlefield-core --release --bin bench-headless
	qemu-aarch64 -L /usr/aarch64-linux-gnu $(ROOT)/target/$(TARGET_ARM64)/release/bench-headless

# --- QEMU System Emulation ---

qemu-setup:
	./qemu-system/setup-image.sh

qemu-launch:
	./qemu-system/launch.sh

qemu-provision:
	./qemu-system/provision.sh
