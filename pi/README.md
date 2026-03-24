# Raspberry Pi / ARM Testing

Test the battlefield game on ARM hardware using QEMU emulation and cross-compilation.

## Prerequisites

```bash
# Cross-compilation via Docker (recommended)
cargo install cross --locked
# Docker must be installed and running

# Manual cross-compilation (alternative, no Docker)
sudo apt-get install gcc-aarch64-linux-gnu gcc-arm-linux-gnueabihf
rustup target add aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf

# QEMU user-mode (run cross-compiled binaries on host)
sudo apt-get install qemu-user-static

# QEMU system emulation (full VM)
sudo apt-get install qemu-system-arm qemu-efi-aarch64 wget
```

## Quick Start

```bash
cd pi

# Cross-compile for Pi 4 (aarch64)
make build-arm64

# Run headless benchmark under QEMU user-mode
make bench-arm64

# Run Criterion benchmarks natively
make bench
```

## Cross-Compilation

Uses [cross](https://github.com/cross-rs/cross) with Docker to handle the ARM toolchain and SDL2 dependencies automatically.

```bash
# Pi 3/4/5 (64-bit ARM)
make build-arm64

# Pi Zero/2W (32-bit ARM)
make build-armv7

# Run the ARM64 binary under QEMU user-mode
make run-arm64
```

The cross-compilation config is in `../Cross.toml`. For manual cross-compilation without Docker, the linker and runner settings are in `../.cargo/config.toml`.

## Benchmarks

### Headless Frame Benchmark

Simulates game frames (tick + update) without rendering. Reports timing statistics and checks against 60fps budget (16.67ms/frame).

```bash
# Run natively
BENCH_FRAMES=3600 cargo run --package battlefield-core --release --bin bench-headless

# Run on ARM via QEMU user-mode
make bench-arm64

# Configure
BENCH_FRAMES=600 BENCH_SEED=123 make bench-arm64
```

### Criterion Benchmarks

Micro-benchmarks for game_tick, game_update, full_frame, and mapgen with statistical analysis.

```bash
make bench
# HTML reports in target/criterion/
```

## QEMU System Emulation (Full VM)

For full-stack testing with SDL2 display, boot a Debian arm64 VM emulating a Pi 4 (Cortex-A72, 1GB RAM, 4 cores).

```bash
# 1. Download Debian arm64 cloud image
make qemu-setup

# 2. Boot the VM
make qemu-launch

# 3. SSH in and install Rust + SDL2 deps
make qemu-provision

# 4. Copy the project and build inside the VM
scp -P 2222 -r .. root@localhost:~/the-battlefield/
ssh -p 2222 root@localhost
cd ~/the-battlefield && cargo build --package battlefield-sdl --release
```

### VM Configuration

Adjust the VM profile via environment variables:

```bash
RAM=2048 CPUS=4 SSH_PORT=2222 make qemu-launch

# Headless (no display window)
DISPLAY_MODE=none make qemu-launch
```

## Target Reference

| Target | Architecture | Pi Models |
|--------|-------------|-----------|
| `aarch64-unknown-linux-gnu` | ARM64 | Pi 3, 4, 5 |
| `armv7-unknown-linux-gnueabihf` | ARMv7 | Pi Zero 2W, older |

## Performance Notes

- **QEMU user-mode** runs ~10-30x slower than native. Use for regression testing (relative comparisons), not absolute numbers.
- **Real Pi 4** (Cortex-A72 @ 1.8GHz) is roughly 3-5x slower than modern x86 for single-threaded workloads.
- The headless benchmark measures game logic only. SDL2 rendering adds additional load on real hardware.
