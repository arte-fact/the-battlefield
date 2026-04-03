#!/bin/bash
set -e

echo "Building battlefield-wgpu-native for wasm32-unknown-unknown..."
cargo build -p battlefield-wgpu-native --target wasm32-unknown-unknown --release

OUT="web-wgpu"
mkdir -p "$OUT"

echo "Running wasm-bindgen..."
wasm-bindgen --out-dir "$OUT" --target web --no-typescript \
    target/wasm32-unknown-unknown/release/battlefield_wgpu_native.wasm

# Copy PWA assets
cp manifest.json "$OUT/manifest.json"
mkdir -p "$OUT/icons"
cp icons/*.png "$OUT/icons/"

# Create service worker with cache-busted name from wasm hash
WASM_HASH=$(sha256sum "$OUT/battlefield_wgpu_native_bg.wasm" | cut -c1-8)
sed "s/battlefield-f0c6fdfa/battlefield-wgpu-${WASM_HASH}/" sw.js > "$OUT/sw.js"
# Update precache list for wgpu file names
sed -i "s|./battlefield.js|./battlefield_wgpu_native.js|" "$OUT/sw.js"
sed -i "s|./battlefield.wasm|./battlefield_wgpu_native_bg.wasm|" "$OUT/sw.js"

echo ""
echo "Build complete: $OUT/"
echo "Serve with: python3 -m http.server -d $OUT 8090"
