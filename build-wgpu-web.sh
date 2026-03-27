#!/bin/bash
set -e

echo "Building battlefield-wgpu-native for wasm32-unknown-unknown..."
cargo build -p battlefield-wgpu-native --target wasm32-unknown-unknown --release

OUT="web-wgpu"
mkdir -p "$OUT"

echo "Running wasm-bindgen..."
wasm-bindgen --out-dir "$OUT" --target web --no-typescript \
    target/wasm32-unknown-unknown/release/battlefield_wgpu_native.wasm

# Create HTML shell if it doesn't exist
if [ ! -f "$OUT/index.html" ]; then
cat > "$OUT/index.html" << 'HTMLEOF'
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>The Battlefield (wgpu)</title>
    <style>
        body { margin: 0; background: #1a1a26; overflow: hidden; }
        canvas { display: block; width: 100vw; height: 100vh; }
    </style>
</head>
<body>
    <canvas id="canvas"></canvas>
    <script type="module">
        import init from './battlefield_wgpu_native.js';
        await init();
    </script>
</body>
</html>
HTMLEOF
fi

echo ""
echo "Build complete: $OUT/"
echo "Serve with: python3 -m http.server -d $OUT 8090"
