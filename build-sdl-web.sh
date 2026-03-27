#!/bin/bash
set -e

# Build the SDL crate for Emscripten (web).
# Requires: Emscripten SDK activated (source emsdk_env.sh)

if [ -z "$EMSDK" ]; then
    echo "Error: EMSDK not set. Run 'source /path/to/emsdk/emsdk_env.sh' first."
    exit 1
fi

echo "Building battlefield-emscripten for wasm32-unknown-emscripten..."
cargo build --target wasm32-unknown-emscripten --release \
    -p battlefield-emscripten --no-default-features

OUT="web-sdl/dist"
mkdir -p "$OUT"

RELEASE="target/wasm32-unknown-emscripten/release"
cp "$RELEASE/battlefield.js" "$OUT/"
cp "$RELEASE/battlefield.wasm" "$OUT/"
cp web-sdl/shell.html "$OUT/index.html"

# Copy PWA infrastructure
cp manifest.json "$OUT/"
cp -r icons "$OUT/icons" 2>/dev/null || true

# Cache-busting: update sw.js CACHE_NAME with hash of wasm binary
cp sw.js "$OUT/sw.js"
if command -v sha256sum &>/dev/null; then
    HASH=$(sha256sum "$OUT/battlefield.wasm" | cut -c1-8)
    CACHE_NAME="battlefield-${HASH}"
    sed -i "s/const CACHE_NAME = '.*';/const CACHE_NAME = '${CACHE_NAME}';/" "$OUT/sw.js"
    echo "Cache: ${CACHE_NAME}"
fi

echo ""
echo "Build complete: $OUT/"
echo "Serve with: python3 -m http.server -d $OUT 8080"
