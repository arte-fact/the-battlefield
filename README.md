# The Battlefield

A roguelike, turn-based tactics game set on a medieval battlefield. You are one soldier in a massive battle between two armies. Survive. Fight. Turn the tide.

Built for the web with Rust, WebAssembly, and HTML Canvas 2D. Mobile-first, playable offline as a PWA.

## About

The Battlefield is a permadeath roguelike where each run places you as a single soldier in a procedurally generated battle between two organized medieval armies. You don't control the army -- you follow orders, fight nearby enemies, and try to survive while contributing to your side's victory.

Every battle is different: terrain, faction pairings, army composition, and commander strategies are all procedurally generated. When you die, you start over in a new battle with new conditions.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust |
| Compilation target | WebAssembly |
| Rendering | HTML Canvas 2D |
| Input | Touch-first (swipe, tap, pinch) + keyboard/mouse fallback |
| Offline support | PWA with service worker |
| Deployment | GitHub Pages via GitHub Actions |
| Art | [Tiny Swords](https://pixelfrog-assets.itch.io/tiny-swords) by Pixel Frog (itch.io) |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
- Any modern browser (Chrome, Firefox, Safari, Edge)

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
- [ ] HTML Canvas 2D rendering pipeline (initialize canvas, basic draw)
- [ ] Canvas setup and game loop (fixed timestep)
- [ ] GitHub Actions CI/CD pipeline (build, test, clippy, fmt)
- [ ] GitHub Pages deployment
- [ ] Sprite sheet loader (parse horizontal strip PNGs into frames)
- [ ] Render a single animated unit (Warrior idle) on screen

### Phase 2: Core Gameplay

- [ ] 64x64 square grid map with tilemap rendering (Tiny Swords tilesets)
- [ ] Camera controls (pan, zoom)
- [ ] Turn system (player turn / AI turn)
- [ ] Unit placement and movement on the grid (with dust particle FX)
- [ ] Sprite facing (horizontal flip for left-facing units)
- [ ] Basic melee combat (Warrior attack animation + explosion FX on hit)
- [ ] Ranged combat (Archer shoot animation + arrow projectile)
- [ ] Health system with HP bars (BigBar/SmallBar UI assets)
- [ ] Unit death (explosion FX + fade out)
- [ ] Touch input: swipe-anywhere movement & attack (8-directional with pathfinding auto-move)
- [ ] Touch input: tap End Turn button
- [ ] Touch input: pinch-to-zoom, two-finger-pan
- [ ] Responsive canvas (fill viewport on mobile)
- [ ] On-screen End Turn button

### Phase 3: Battlefield and Armies

- [ ] Procedural terrain generation (grass, elevation, water, trees, rocks, bushes)
- [ ] Building placement (castles, towers, houses, barracks, monastery)
- [ ] Two-faction army generation (select from 5 faction colors)
- [ ] All 5 unit types functional (Warrior, Archer, Lancer, Pawn, Monk)
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
