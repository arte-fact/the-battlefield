# The Battlefield

A roguelike, turn-based tactics game set on a medieval battlefield. You are one soldier in a massive battle between two armies. Survive. Fight. Turn the tide.

Built with Rust, WebAssembly, and SDL2. Runs natively (desktop/ARM) and on the web via Emscripten (WebGL). Mobile-first, playable offline as a PWA.

## About

The Battlefield is a permadeath roguelike where each run places you as a single soldier in a procedurally generated battle between two organized medieval armies. You don't control the army -- you follow orders, fight nearby enemies, and try to survive while contributing to your side's victory.

Every battle is different: terrain, faction pairings, army composition, and commander strategies are all procedurally generated. When you die, you start over in a new battle with new conditions.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust |
| Web target | WebAssembly via Emscripten |
| Native target | SDL2 (Linux, ARM/Raspberry Pi) |
| Rendering | SDL2 → WebGL (web) / GPU-accelerated (native) |
| Input | Touch (joystick, buttons, pinch) + keyboard/mouse + gamepad |
| Offline support | PWA with service worker |
| Deployment | GitHub Pages via GitHub Actions |
| Art | [Tiny Swords](https://pixelfrog-assets.itch.io/tiny-swords) by Pixel Frog (itch.io) |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [cmake](https://cmake.org/) (required by the SDL2 bundled build)
- Any modern browser (Chrome, Firefox, Safari, Edge)

### Build and Run (Native SDL2)

```bash
cargo run -p battlefield-sdl
```

### Build and Run (Web via Emscripten)

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

The build script compiles the SDL crate to WebAssembly via Emscripten, bundles
game assets into a `.data` file, applies service worker cache-busting, and
copies PWA files into `web-sdl/dist/`.

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

## Roadmap

### Phase 1: Foundation

- [x] Rust project setup with `Cargo.toml` and WASM target
- [x] HTML Canvas 2D rendering pipeline (initialize canvas, basic draw)
- [x] Canvas setup and game loop (fixed timestep)
- [ ] GitHub Actions CI/CD pipeline (build, test, clippy, fmt)
- [ ] GitHub Pages deployment
- [x] Sprite sheet loader (parse horizontal strip PNGs into frames)
- [x] Render a single animated unit (Warrior idle) on screen

### Phase 2: Core Gameplay

- [x] 64x64 square grid map with tilemap rendering (Tiny Swords tilesets)
- [x] Camera controls (pan, zoom, smooth follow)
- [x] Turn system (auto-turn: player acts, then AI acts, turn advances)
- [x] Unit placement and movement on the grid (with dust particle FX)
- [x] Sprite facing (horizontal flip for left-facing units)
- [x] Basic melee combat (Warrior attack animation + explosion FX on hit)
- [x] Ranged combat (Archer shoot animation + arrow projectile)
- [x] Health system with HP bars
- [x] Unit death (explosion FX + fade out)
- [x] Touch input: swipe-anywhere movement & attack (short swipe = 1 tile, long swipe = A* pathfinding auto-move)
- [x] Touch input: pinch-to-zoom, two-finger-pan
- [x] Responsive canvas (fill viewport on mobile, DPR scaling)
- [x] Keyboard: arrow keys = movement, WASD = camera pan, mouse wheel = zoom

### Phase 3: Battlefield and Armies

- [x] Procedural terrain generation (grass, elevation, water, forest, rock)
- [x] 4-bit cardinal bitmask auto-tiling (flat ground + elevated ground)
- [x] Water rendering with animated foam edges
- [x] Elevation rendering with shadows and cliff faces
- [x] All 5 unit types functional (Warrior, Archer, Lancer, Pawn, Monk)
- [ ] Building placement (castles, towers, houses, barracks, monastery)
- [ ] Two-faction army generation (select from 5 faction colors)
- [ ] Lancer: larger sprite (320x320), directional attack/defence, charge ability
- [ ] Monk: heal animation + heal effect overlay on target
- [ ] Pawn: tool variant animations as melee attack
- [ ] Army hierarchy (army, divisions, squads)
- [ ] AI commanders with portraits (25 avatars) issuing orders
- [ ] Squad-level AI (units following orders, engaging enemies)
- [ ] Player receives and responds to orders
- [ ] Morale system (units break and flee, Monks rally)

### Phase 4: Roguelike Loop

- [ ] Permadeath (run ends on player death, death screen with avatar)
- [ ] Battle end conditions (victory, defeat, rout)
- [ ] Run summary screen (stats on RegularPaper background, Swords decoration)
- [ ] Procedural variety (faction pairing, terrain layout, army composition, tilemap color variant)
- [ ] New run setup (battle generation with new conditions)
- [ ] Meta-progression system (TBD -- unlockable roles, scenarios)

### Phase 5: Polish

- [ ] PWA manifest and service worker (offline play)
- [ ] Full UI with asset pack components (buttons, banners, ribbons, cursors, papers, wood table)
- [ ] Main menu (Banner title, WoodTable background, Blue/Red buttons)
- [ ] In-battle HUD (health bar, morale bar, orders ribbon, action icons, minimap)
- [ ] Battle overview toggle (zoomed-out army positions)
- [ ] Cloud shadows drifting across battlefield
- [ ] Animated decorations (swaying trees, bushes, water rocks)
- [ ] Sound effects (combat, movement, orders, ambient)
- [ ] Music
- [ ] Mobile-optimized HUD layout (bottom action bar)
- [ ] Haptic feedback on attack/damage (Vibration API)
- [ ] Touch target sizing validation (44px minimum)

### Phase 6: Content and Balance

- [ ] Unit ability balancing (Guard, Volley, Charge, Brace, Heal)
- [ ] Terrain and building defense bonus tuning
- [ ] Commander AI personality variety and balancing
- [ ] Army composition templates (infantry-heavy, cavalry-heavy, balanced, skirmish)
- [ ] Battle scenario variety
- [ ] Playtesting and tuning

## Documentation

- [Game Design Document](docs/gdd.md)
- [Asset Pack Reference](docs/asset-pack.md)

## License

TBD
