# The Battlefield

A roguelike, turn-based tactics game set on a medieval battlefield. You are one soldier in a massive battle between two armies. Survive. Fight. Turn the tide.

Built for the web with Rust, WebAssembly, and WebGPU. Playable offline as a PWA.

## About

The Battlefield is a permadeath roguelike where each run places you as a single soldier in a procedurally generated battle between two organized medieval armies. You don't control the army -- you follow orders, fight nearby enemies, and try to survive while contributing to your side's victory.

Every battle is different: terrain, weather, army composition, and commander strategies are all procedurally generated. When you die, you start over in a new battle with new conditions.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust |
| Compilation target | WebAssembly |
| Rendering | WebGPU |
| Offline support | PWA with service worker |
| Deployment | GitHub Pages via GitHub Actions |
| Art | Top-down animated sprite asset pack (itch.io) |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
- A WebGPU-compatible browser (Chrome 113+, Firefox 121+, Edge 113+)

### Build and Run

```bash
# Build the WASM package
wasm-pack build --target web

# Serve locally (use any static file server)
python3 -m http.server 8080
```

### Tests

```bash
# Run unit tests
cargo test

# Run WASM tests in headless browser
wasm-pack test --headless --chrome
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

- [ ] Rust project setup with `Cargo.toml` and WASM target
- [ ] WebGPU rendering pipeline (initialize device, basic draw)
- [ ] Canvas setup and game loop (fixed timestep)
- [ ] GitHub Actions CI/CD pipeline
- [ ] GitHub Pages deployment
- [ ] Basic sprite rendering (load and display a texture)

### Phase 2: Core Gameplay

- [ ] Square grid map with tile rendering
- [ ] Turn system (player turn / AI turn)
- [ ] Unit movement on the grid
- [ ] Basic melee combat (attack adjacent unit)
- [ ] Health and damage system
- [ ] Unit death and removal
- [ ] Camera controls (pan, zoom)

### Phase 3: Battlefield and Armies

- [ ] Procedural terrain generation (hills, forests, rivers, fortifications)
- [ ] Army generation (two opposing armies with varied composition)
- [ ] Army hierarchy (army, divisions, squads)
- [ ] AI commanders issuing orders (advance, hold, flank, retreat)
- [ ] Squad-level AI (units following orders, engaging enemies)
- [ ] Player receives and responds to orders
- [ ] Morale system (units can break and flee)

### Phase 4: Roguelike Loop

- [ ] Permadeath (run ends on player death)
- [ ] Battle end conditions (victory, defeat, rout)
- [ ] Run summary screen (stats, performance)
- [ ] Procedural variety (weather, time of day, army composition)
- [ ] New run setup (role selection, battle generation)
- [ ] Meta-progression system (TBD)

### Phase 5: Polish

- [ ] PWA manifest and service worker (offline play)
- [ ] Sprite asset integration (animated top-down units)
- [ ] Sound effects (combat, ambient battlefield)
- [ ] Music
- [ ] UI: HUD (health, morale, orders, minimap)
- [ ] UI: Menus (main menu, pause, settings)
- [ ] UI: Battle overview (army positions, front line)

### Phase 6: Content and Balance

- [ ] Unit variety (swordsman, spearman, archer, cavalry, siege)
- [ ] Unit abilities and special actions
- [ ] Terrain effect balancing
- [ ] Commander AI personality types
- [ ] Battle scenario variety
- [ ] Playtesting and tuning

## Documentation

- [Game Design Document](docs/gdd.md)

## License

TBD
