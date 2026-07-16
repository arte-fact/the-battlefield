# The Battlefield

A real-time battle game set on a medieval battlefield. You are one soldier in a massive battle between two armies. Survive. Fight. Earn authority. Turn the tide.

Built with Rust and WebAssembly, with two renderer backends: wgpu (native + web, the deployed target) and SDL2 (desktop/ARM + Emscripten web). Mobile-first, playable offline as a PWA.

## About

The Battlefield is a permadeath game where each run places you as a single soldier in a procedurally generated real-time battle between two armies. You don't control the army -- both sides fight autonomously, capturing zones and sending reinforcement waves. By fighting well you earn authority, and allied soldiers who respect you will follow your orders (Follow, Charge, Defend).

A faction wins by holding all seven capture zones for 60 continuous seconds. When you die, the run ends -- start over on a freshly generated battlefield.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (workspace: headless core + thin platform crates) |
| Web target (deployed) | wgpu + winit → WebAssembly via wasm-bindgen (WebGL) |
| Web target (alternative) | SDL2 → WebAssembly via Emscripten |
| Native target | wgpu (desktop) and SDL2 (desktop, ARM/Raspberry Pi) |
| Input | Touch (joystick, buttons, pinch) + keyboard/mouse + gamepad |
| Offline support | PWA with service worker (cache-busted by wasm hash) |
| Deployment | GitHub Pages via GitHub Actions (wgpu web build) |
| Art | [Tiny Swords](https://pixelfrog-assets.itch.io/tiny-swords) by Pixel Frog (itch.io) |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- For the SDL2 bundled build: `cmake` and X11/audio dev headers on Linux
- For the wgpu native build: `libudev` dev package (gamepad support via gilrs)
- Any modern browser (Chrome, Firefox, Safari, Edge)

### Build and Run (Native)

```bash
# wgpu renderer (same backend as the deployed web version)
cargo run -p battlefield-wgpu-native

# SDL2 renderer (desktop / Raspberry Pi)
cargo run -p battlefield-sdl
```

### Build and Run (Web, wgpu -- deployed target)

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.117

./build-wgpu-web.sh

# Serve locally
python3 -m http.server -d web-wgpu 8080
```

### Build and Run (Web, SDL via Emscripten)

```bash
# One-time setup: install Emscripten SDK
git clone https://github.com/emscripten-core/emsdk.git ~/emsdk
cd ~/emsdk && ./emsdk install latest && ./emsdk activate latest
rustup target add wasm32-unknown-emscripten

# Before each build session
source ~/emsdk/emsdk_env.sh

# Build
./build-sdl-web.sh

# Serve locally
python3 -m http.server -d web-sdl/dist 8080
```

Both web build scripts bundle game assets, apply service worker cache-busting, and copy PWA files into their output directory.

### Tests

```bash
cargo test
```

### Linting

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

## Development Guidelines

- **TDD** -- Write tests first. Every feature starts with a failing test.
- **SOLID** -- Single responsibility, open/closed, Liskov substitution, interface segregation, dependency inversion.
- **Small files** -- Split files when they grow too large. Each file should have a clear, focused purpose.
- **Low complexity** -- Keep functions short. Minimize nesting depth. Extract early returns.
- **Idiomatic Rust** -- Follow Rust conventions. Zero clippy warnings. Use `cargo fmt`.
- **No anticipation** -- Don't code for hypothetical future needs (YAGNI). Build what's needed now.

## Current State

Implemented:

- Real-time simulation with hundreds of units (flow fields, budgeted A*, spatial hashing)
- Four unit types (Warrior, Archer, Lancer, Monk) with distinct combat behaviors
- Seven capture zones with a faction-level AI planner and zone-control victory
- Bases with producing buildings and reinforcement waves
- Authority system: earn reputation, command allies with Follow / Charge / Defend orders
- Procedural battlefields (seeded BSP + simplex terrain, auto-tiling, decorations)
- Fog of war, minimap, touch/keyboard/gamepad input, PWA offline play
- CI (fmt, clippy, tests, web builds) and automated GitHub Pages deployment

Not yet implemented (see [Future Directions](docs/gdd.md#future-directions)):

- Sound and music
- Morale / rout mechanics
- Meta-progression and run summary screen
- Additional faction pairings and commander personalities

## Documentation

- [Game Design Document](docs/gdd.md)
- [Asset Pack Reference](docs/asset-pack.md)

## License

TBD
