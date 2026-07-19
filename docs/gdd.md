# The Battlefield -- Game Design Document

> This document describes the game as currently implemented. Ideas that were
> part of earlier designs but are not (or no longer) in the game are collected
> in [Future Directions](#future-directions) at the end.

## Game Overview

**Title:** The Battlefield
**Genre:** Real-time action / battle sim with a roguelike run structure
**Platform:** Web (PWA, mobile-first, playable offline), desktop Linux, Raspberry Pi
**Perspective:** Top-down, square grid
**Art:** Tiny Swords asset pack (Pixel Frog) -- chibi pixel art, 64x64 tile grid

**Elevator pitch:** You are one soldier in a massive medieval battle between two armies. Survive. Fight. Earn authority. Turn the tide. Die. Try again.

**Core fantasy:** You're not the general. You're not the hero -- at least not at first. You're a soldier in the ranks, fighting for your life in a real-time battle much larger than yourself. The armies clash around you whether you act brilliantly or not -- but by fighting well you build a reputation, and soldiers who have seen your deeds will follow your orders.

## Core Pillars

### 1. One Soldier, One Life

Every run is one soldier's experience of one battle. Permadeath means every decision carries weight. When you fall, the battle continues without you.

### 2. The Battle Is Bigger Than You

Both armies fight autonomously: a faction-level AI planner picks objectives, reinforcement waves march from the bases, and capture zones change hands with or without you. Your actions influence your local area -- and through the authority system, increasingly more than that.

### 3. Authority Is Earned

You start unknown. Kills, assists, and zone captures witnessed by nearby allies raise your authority; allied deaths near you lower it. The higher your authority, the more soldiers will accept your orders, and the farther your command reaches.

## Game Loop

### Run Structure

1. **Battle generation** -- A seeded, deterministic map is generated: terrain, two bases at opposing corners, and seven capture zones.
2. **Deployment** -- You spawn as a Blue Warrior at your base, alongside your army's starting force. The Red army spawns at the opposite corner.
3. **Battle** -- Real-time gameplay. Fight, capture zones, issue orders to allies who respect you.
4. **Resolution** -- The run ends when you die (permadeath) or one faction wins: by **domination** (holding all seven zones for 60 continuous seconds) or by **annihilation** (the enemy has no manpower left and no living units).
5. **New run** -- Retry from the death/victory/defeat screen; a new seed produces a new battlefield.

### Screens

`MainMenu → Playing → PlayerDeath | GameWon | GameLost → (retry / new game)`

## Map and Battlefield

### Grid

The world is a 192x192 tile grid at 64px per tile: a 160x160 playable area surrounded by a 16-tile impassable border (the playable size is a config value; 160 is the default). Positions are continuous (floating point); the grid governs terrain, passability, pathfinding, and auto-tiling. Units render in 192x192 frames (the mounted Lancer in 320x320), Y-sorted for correct overlap.

### Terrain

Seeded procedural generation (BSP layout + simplex noise) produces:

- **Grass** -- open ground, auto-tiled with a 4-bit cardinal bitmask.
- **Elevated ground** -- raised terrain with cliff faces and shadows.
- **Water** -- impassable, animated foam edges.
- **Forest** -- tree decorations; pawns chop trees for ambience.
- **Rocks** -- impassable decorations.

The same seed always generates the same battlefield; different seeds differ.

### Bases

Each faction owns a base in its corner of the map, laid out by BSP partitioning and connected by dirt roads. The base **faces the enemy**: layouts are seeded functional bands rotated to the base-to-base axis -- castle and 3-5 defense towers on the front arc, one production building per wave unit kind on the flanks, 8-12 houses and the sheep pasture in the rear:

| Building | Role |
|----------|------|
| Castle | Defensive anchor -- fires arrows like 4 archers |
| Defense Tower | Defensive -- fires like 2 archers |
| Barracks | Produces Warriors and Lancers |
| Archery Range | Produces Archers |
| Monastery | Produces Monks |
| Houses | Decorative; pawns and sheep live around them |

### Capture Zones are Villages

Seven named capture zones are arranged in a diamond layout across the battlefield, linked by a symmetric adjacency graph that the AI uses for planning. Zone centers nudge within a small window to spare lakes and cliffs, with mirrored offsets so both sides stay equidistant. Each zone tracks the units inside it and moves a capture progress value between fully Red (-1) and fully Blue (+1).

Every zone is a small **village** with one worked resource, themed per seed (all three themes appear on every map):

| Theme | Resource | Peons | Production building |
|-------|----------|-------|---------------------|
| Mining camp | Gold stones (impassable) | Pickaxe, carry gold | Barracks (warriors) |
| Lumber camp | Tree grove | Axe, carry wood | Archery (archers) |
| Pasture | Sheep pen | Knife, carry meat | Monastery (monks) |

Each village has 2-3 houses (one peon each), its production building (the center zone rolls a second), and the defense tower at its heart. Buildings and peons are **Black (neutral) until captured**, then recolor to the owner's faction.

**Village economy:** each peon delivery banks 1 stock (cap 5). A controlled village adds its building's units to the owner's reinforcement wave, **spawned at the village** -- front-line reinforcements are the payoff of holding it. Village units cost normal manpower and 1 stock each; peons **flee combat**, so marching an army through a village scatters its workers and stalls its output without capturing it.

**Majority capture:** progress moves at the rate of the *strength difference* between the factions inside (√|blue − red|). Equal forces freeze the zone; a minority garrison slows an assault but cannot hold forever — overwhelming force completes the capture even with defenders still alive. Attacking a defended point is a readable numbers race, not a binary stall.

Zone states: **Neutral → Contested → Capturing(faction) → Controlled(faction)**.

**Victory:** a faction that controls all seven zones simultaneously starts a 60-second victory timer. If it holds them all for the full duration, it wins by domination. Losing any zone resets the timer. **Sudden death:** once both manpower pools are exhausted, a strict zone majority held for the same duration wins instead.

**Manpower and bleed (Conquest attrition):** each faction starts with a finite manpower pool (default 300) -- the reinforcements it can still field. Every reinforcement spawn costs 1; the starting armies are free. Controlling a majority of zones (4+ of 7) bleeds the enemy pool over time, scaling with each zone at or above the threshold. A faction whose pool is empty and whose army is destroyed loses by annihilation -- so any sustained advantage ends the battle eventually, and every kill is a manpower point the enemy must spend to replace.

## Units

Two factions -- **Blue** (the player's side) and **Red** -- field the same four unit types:

| Unit | Role | HP | ATK | DEF | Range (tiles) |
|------|------|:---:|:---:|:---:|:---:|
| Warrior | Frontline melee, balanced attack and defense | 10 | 3 | 3 | 1 |
| Archer | Ranged attacker, fragile up close, kites melee | 6 | 2 | 1 | 7 |
| Lancer | Fast cavalry, hits hard, backs off after striking | 10 | 4 | 1 | 2 |
| Monk | Healer -- avoids combat, flees enemies, heals nearby allies | 5 | 1 | 1 | 2 |

Stats live in `GameConfig` and can be rebalanced at runtime. Damage is `max(1, ATK - DEF)`. Dead units explode into particles and fade out; the player's corpse stays on the field.

## The Player

You play a Blue Warrior in real time:

- **Movement** -- continuous 360° movement via virtual joystick (touch), WASD/arrows (keyboard), or gamepad stick.
- **Attack** -- an explicit attack action (button / Space / gamepad) that hits all enemies in a 180° cone in your facing direction, with knockback. No auto-attack. **You fight exactly like other soldiers**: AI melee units use the same cone swing, knockback, and reach (1.5 tiles), at the same rate.
- **Fog of war** -- you see a personal field-of-view radius; the rest of the map is fogged (FOV recomputed every third frame for performance).

### Authority

Authority is a 0-100 reputation score, displayed as a rank:

| Authority | Rank |
|:---:|------|
| 0+ | Unknown |
| 20+ | Known |
| 40+ | Veteran |
| 60+ | Hero |
| 80+ | Legend |

It changes in response to events **witnessed within your reputation FOV** -- kills and assists you land, zones captured while you're present (positive); allied deaths nearby, zones lost (negative). Authority determines three things, each scaling linearly with the score:

1. **Recruitment chance** -- the probability an allied unit joins your retinue.
2. **Command radius** -- how far around you recruitment reaches.
3. **Max followers** -- how many soldiers your retinue can hold.

### The Retinue (auto-follow)

Soldiers join you on their own. Every second, allied units inside your command radius with no current assignment roll a deterministic acceptance check (same unit + same authority level = same answer, so there is nothing to reroll); accepters become **sticky followers** up to your follower cap. Failures are silent. Followers stay yours until they die, you die, you Dismiss them, or they lose contact (left more than 15 tiles behind for a few seconds).

The pull is always on -- walk past a zone your side is defending and you will vacuum its garrison into your wake. Managing where you walk *is* a tactical decision, and delivering your retinue somewhere and dismissing it is a strategic one (troop ferrying).

### Orders

Charge and Defend command **your retinue** -- these soldiers already chose you, so they always obey, subject to commitment:

| Order | Key | Effect | Ends |
|-------|:---:|--------|------|
| Charge | J | Followers rush a point ahead of you, then revert to Follow | on arrival or timeout |
| Defend | K | Followers form a layered line at your position (Warriors front, Lancers, Archers, Monks behind), then revert to Follow | after a timer |
| Dismiss | L (hold on touch, ~0.4s) | Releases the whole retinue back to the army; each unit refuses re-recruitment for 12s | -- |

**Commitment is tied to action timing, per soldier**: a unit sprinting a charge or walking into its defend slot cannot be re-tasked until the move completes; a follower at your side or a defender posted in line obeys instantly. No artificial cooldowns -- pacing comes from real movement time. Timed orders that expire revert the unit to Follow (it stays yours and returns). Ordered units show a marker (progress bar = remaining time; full bar = Follow). Units on orders fight enemies within a leash of their assignment and always defend themselves in melee.

## Army AI

### Faction planner

Each faction periodically scores all seven zones with a 3-tier objective system and assigns its units across the top targets (scoring is staggered between factions to spread the cost across frames). A faction holding zero zones focuses its entire force on a single zone -- a desperation push -- rather than spreading thin.

### Unit behavior

- **Flow fields** -- each faction maintains flow fields toward its objectives; units steer by blending flow direction with local separation to avoid clumping.
- **A\* pathfinding** -- used for individual paths with a per-tick budget and repath cooldowns so hundreds of units stay cheap.
- **Combat** -- units engage any visible enemy: melee units close in, archers kite with hysteresis, lancers strike and back off, monks flee and heal. Target commitment timers prevent flip-flopping between targets.
- **Spatial hash** -- a per-tick spatial grid provides amortised O(1) neighbour queries for separation and enemy searches.

### Reinforcements

Bases produce units in waves, drawing from the faction's manpower pool. Newly produced units rally at the base until the wave is complete, then march together (a wave cut short by an empty pool still marches). A faction holding zero zones produces double-size waves to fuel its comeback -- burning its pool twice as fast, an all-in gamble.

## Ambient Life

Base villages have wandering **pawns** chopping trees and **sheep** grazing the rear pasture -- atmosphere only. Capture-zone peons do the same work loops (chop, mine, herd) but their deliveries feed the village economy above; they are invulnerable and panic away from any nearby fighting.

## Controls

**Arcade cabinet format**: one joystick + a standard 4-button layout. Touch is the primary input; keyboard/mouse and gamepad map to the same scheme.

| Input | Touch | Keyboard / Mouse | Gamepad |
|-------|-------|------------------|---------|
| Move | Virtual joystick | WASD / arrows | Left stick / D-pad |
| Attack | Attack button (held = AI attack rate) | Space | South button |
| Charge | C button | J | West button |
| Defend | D button | K | North button |
| Dismiss | X button (hold, fill ring) | L | East button |
| Zoom | Pinch | Mouse wheel | Right trigger |
| Camera | Follows player | Follows player | Follows player |

## User Interface

- **HUD** -- player health, authority rank, retinue counter (current/cap), zone ownership summary, and per-faction manpower counters (tinted amber while a pool is bleeding).
- **Minimap** -- top-right corner (240px), showing terrain, zones, and unit positions in faction colors.
- **Order markers** -- floating marker with progress bar above units currently under your command; a command-radius pulse ripples out when an order lands.
- **Floating text** -- authority gains/losses pop above the player.
- **Menus** -- main menu, death, victory, and defeat screens built from the Tiny Swords UI kit.
- Mobile-first: fullscreen toggle, DPR-aware canvas scaling, touch targets sized for thumbs.

## Visual Style

Top-down chibi pixel art from the Tiny Swords pack -- colorful, readable, charming rather than gritty. Strong faction color coding (Blue vs Red). Draw order: water, ground, foam, decorations, building bases, Y-sorted units, trees/building tops, particles, projectiles, UI.

## Technical Architecture

### Workspace layout

| Crate | Responsibility |
|-------|---------------|
| `battlefield-core` | All game logic -- simulation, AI, mapgen, zones, input abstraction. Headless, fully testable, no graphics dependencies. |
| `battlefield-assets` | Asset manifest and loading support. |
| `battlefield-sdl` + `battlefield-native` | SDL2 renderer and desktop/ARM entry point. |
| `battlefield-emscripten` | SDL web build (WebGL via Emscripten). |
| `battlefield-wgpu` + `battlefield-wgpu-native` | wgpu/winit renderer -- native and web (wasm-bindgen). **This is the deployed web target.** |

### Core modules

| Module | Responsibility |
|--------|---------------|
| `game/` | Tick loop, faction AI, orders, authority, combat, FOV, player control, setup |
| `mapgen/` | Seeded BSP + simplex terrain generation, 7-zone layout |
| `zone.rs` | Capture zones, adjacency, scoring, victory timer |
| `flowfield.rs` | Per-faction flow fields |
| `grid.rs` | Tile grid, passability, A* pathfinding |
| `autotile.rs` | 4-bit cardinal bitmask auto-tiling |
| `unit.rs` / `combat.rs` | Unit types, stats, damage |
| `building.rs` / `pawn.rs` / `sheep.rs` | Bases, production, ambient life |
| `touch_input.rs` / `player_input.rs` | Platform-agnostic input primitives |
| `camera.rs` / `particle.rs` / `animation.rs` / `rendering/` | Presentation support shared by both renderers |

### Performance

- Real-time simulation with hundreds of units at 60 FPS on mid-range mobile hardware.
- Per-tick A* budget, repath cooldowns, staggered AI scheduling, spatial hashing, throttled FOV.
- Criterion benchmarks (`game_tick`) and a headless frame benchmark binary track regressions.

### Delivery

- PWA with a service worker (cache-busted by wasm hash) for offline play.
- GitHub Actions: CI (fmt, clippy, tests, web builds) and automated GitHub Pages deployment of the wgpu web build.

## Future Directions

Ideas from earlier designs and open threads, not currently implemented:

- **Sound and music** -- combat SFX, ambient battle noise, dynamic intensity music.
- **Morale** -- units breaking and fleeing, rallying, routs as an alternate battle end.
- **Meta-progression** -- unlockable starting roles (Archer, Lancer, Monk), scenarios, or starting conditions. Should preserve the principle that skill matters more than accumulated power.
- **Run summary** -- post-battle stats screen (kills, zones captured, peak authority).
- **More factions** -- the asset pack provides 5 faction colors; battles currently use Blue vs Red.
- **Commander personalities** -- varied strategic profiles for the enemy faction planner.
- **Battle overview** -- a zoomed-out tactical view beyond the minimap.
