#!/bin/bash
# Build wasm and update service worker cache version.
set -e

echo "Building wasm..."
wasm-pack build crates/client --target web --out-dir ../../pkg

# Generate cache-busting hash from the wasm binary
HASH=$(sha256sum pkg/battlefield_client_bg.wasm | cut -c1-8)
CACHE_NAME="battlefield-${HASH}"

# Update CACHE_NAME in sw.js (the browser byte-diffs sw.js on each navigation;
# a changed CACHE_NAME triggers install → precache → activate → purge old caches)
sed -i "s/const CACHE_NAME = '.*';/const CACHE_NAME = '${CACHE_NAME}';/" sw.js

echo "Build complete — cache: ${CACHE_NAME}"
