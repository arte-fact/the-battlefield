# The Battlefield -- Game Design Document

## Game Overview

**Title:** The Battlefield
**Genre:** Roguelike, turn-based tactics
**Platform:** Web (PWA, mobile-first, playable offline)
**Perspective:** Top-down, square grid
**Art:** Tiny Swords asset pack (Pixel Frog) -- chibi pixel art, 64x64 tile grid

**Elevator pitch:** You are one soldier in a massive medieval battle between two armies. Survive. Fight. Turn the tide. Die. Try again.

**Core fantasy:** You're not the general. You're not the hero. You're a soldier in the ranks, following orders, fighting for your life in a battle much larger than yourself. The army clashes around you whether you act brilliantly or not -- but your choices still matter.

## Core Pillars

### 1. One Soldier, One Life

Every run is one soldier's experience of one battle. Permadeath means every decision carries weight. There are no saves, no checkpoints. When you fall, the battle continues without you.

### 2. The Battle Is Bigger Than You

The two armies fight according to their commanders' strategies. Squads advance, hold positions, and retreat based on orders from above. You are part of this machine -- you receive orders, you see the battle unfold around you, but you don't control it. Your actions can influence your local area and sometimes tip the balance, but the outcome depends on the whole army.

### 3. Every Battle Is Different

No two runs are the same. Armies are procedurally generated with different compositions, strengths, and commanders. Terrain varies. Faction pairings change. The battle you fight today is not the battle you'll fight tomorrow.

## Game Loop

### Run Structure

1. **Battle generation** -- The game generates two opposing armies (selecting factions, composition, commanders), a battlefield (terrain layout, objectives), and battle conditions.
2. **Deployment** -- The player is placed in their squad's starting position within the army formation.
3. **Battle** -- Turn-by-turn gameplay. Each turn, the player receives orders, decides their actions, and the battle progresses.
4. **Resolution** -- The run ends when the player dies (permadeath) or the battle concludes (victory, defeat, or rout of either side).
5. **Summary** -- A post-battle screen shows the player's performance: turns survived, enemies defeated, orders followed, contribution to the battle outcome.
6. **New run** -- Return to battle generation with new procedural conditions.

### Turn Structure

Each turn allows **one action** (move OR attack), then auto-ends:

1. **Orders phase** -- The player's squad commander issues or updates orders (advance to position X, hold this line, fall back). Orders are displayed clearly.
2. **Player action phase** -- The player performs one action: move (swipe/click a direction) or attack (swipe toward/tap enemy). The turn auto-ends after a completed move or attack. If an enemy enters attack range during movement, the walk is interrupted and the player may attack before the turn ends.
3. **AI action phase** -- All other units on both sides act simultaneously based on their orders and local AI behavior.
4. **Resolution phase** -- Combat is resolved, morale is updated, units that break begin fleeing, dead units are removed.

## Map and Battlefield

### Grid

The battlefield uses a **64x64 pixel square grid**. Each cell can contain terrain features and at most one unit. Units render in 192x192 frames (3x3 tiles), visually centered on their tile. The Lancer renders in 320x320 frames (5x5 tiles) due to its mount.

### Terrain Tiles

The Tiny Swords tileset provides 5 color variants of ground tiles (grass + stone elevation) plus water tiles. Each tilemap is 576x384 (9x6 tiles at 64x64) and includes auto-tiling pieces (edges, corners, fills) for:

- **Grass** -- Open ground, standard terrain
- **Elevated ground** -- Raised terrain with cliff faces (functions as hills)
- **Water** -- Animated water with foam edges (16-frame animation)

| Terrain | Asset | Movement cost | Defense bonus | Notes |
|---------|-------|:---:|:---:|-------|
| Open field | Grass tiles | 1 | 0 | Default terrain |
| Hill | Elevation tiles | 2 | +1 | Height advantage for ranged attacks |
| Forest | Tree decorations on grass | 2 | +1 | Blocks line of sight. 4 tree variants + stumps. |
| Water | Water tiles + foam | Impassable | -- | Must cross at bridges |
| Bridge | Custom tile or building piece | 1 | 0 | Crossing point over water |
| Rocks | Rock decorations | Impassable | -- | Blocking terrain. 4 variants. |
| Village | House buildings on grass | 1 | +1 | Houses provide partial cover |
| Fortification | Tower / Castle buildings | 1 | +2 | Defensive structures |

### Decorations

Available map decorations from the asset pack:

- **Trees** (4 variants, animated swaying, 8 frames each) -- forest areas
- **Tree stumps** (4 variants, static) -- cleared forest, battlefield debris
- **Bushes** (4 variants, animated) -- light cover, visual variety
- **Rocks** (4 variants, 64x64 static) -- impassable terrain obstacles
- **Water rocks** (4 variants, animated) -- decorations in water tiles
- **Clouds** (8 variants) -- shadow overlays drifting across the battlefield
- **Gold stones** (6 variants) -- decorative battlefield props
- **Sheep** (animated: idle, move, graze) -- ambient livestock in village areas

### Buildings as Battlefield Structures

Buildings serve as key terrain features and objectives:

| Building | Grid footprint | Role on battlefield |
|----------|:---:|-----|
| Castle (320x256) | ~5x4 | Army HQ / primary objective. Capturing or defending wins battles. |
| Tower (128x256) | ~2x4 | Defensive position. Archers inside get range and defense bonus. |
| Barracks (192x256) | ~3x4 | Strategic objective. Reinforcement point. |
| Archery range (192x256) | ~3x4 | Strategic objective. |
| Monastery (192x320) | ~3x5 | Healing point. Monks can rally broken units here. |
| House 1-3 (128x192) | ~2x3 | Village terrain. Provides cover. |

All buildings come in 5 faction colors, making occupied buildings visually indicate control.

### Battlefield Zones

The battlefield is conceptually divided into zones that inform army AI behavior:

- **Frontline** -- Where the two armies meet. Most combat happens here.
- **Flanks** -- The sides of the engagement. Flanking maneuvers target these.
- **Rear** -- Behind the front. Commanders, reserves, and archers.
- **Objectives** -- Key buildings and positions (castles, towers, bridges, hilltops) that commanders order units to capture or defend.

### Procedural Battlefield Generation

Terrain is generated with constraints to ensure playable, interesting battlefields:

- A mostly open center where the main engagement happens
- Terrain features distributed to create tactical variety (flanking routes through forests, defensible hills, water crossings as chokepoints)
- Objectives (buildings) placed at strategically interesting locations
- Both sides have a castle or equivalent HQ in their deployment zone
- Towers and other structures placed at contested positions
- Village clusters near the center or on flanks

## Factions

The asset pack provides **5 distinct factions** identified by color: Blue, Red, Purple, Yellow, and Black. Each faction has identical unit types and buildings but with distinct color palettes.

Each battle selects **two factions** as the opposing armies. The player is assigned to one.

| Pairing | Contrast | Feel |
|---------|----------|------|
| Blue vs Red | High | Classic, default matchup |
| Purple vs Yellow | High | Alternative vibrant matchup |
| Black vs Blue | High | Dark invaders vs defenders |
| Black vs Red | High | Dark army vs crimson army |
| Red vs Purple | Medium | Civil war / rival kingdoms |

The faction pairing is part of procedural generation -- different each run.

## Units and Roles

### Player Character

The player starts each run as a soldier. The starting role determines their stats, animations, and available actions.

**Starting role (default):** Warrior

Additional roles may be unlocked through the meta-progression system (TBD).

### Unit Types

The asset pack provides 5 unit types per faction. Each maps to a distinct battlefield role:

| Unit | Sprite | HP | ATK | DEF | MOV | Range | Role |
|------|--------|:---:|:---:|:---:|:---:|:---:|------|
| Warrior | Sword & shield knight | 10 | 3 | 3 | 3 | 1 | Frontline melee. Balanced attack and defense. Can guard. |
| Archer | Hooded bowman | 6 | 2 | 1 | 3 | 5 | Ranged attacker. Fragile in melee. Arrow projectile. |
| Lancer | Mounted knight | 10 | 4 | 1 | 5 | 1 | Cavalry. Fast, powerful charge. Directional attacks. |
| Pawn | Light worker/militia | 7 | 2 | 1 | 4 | 1 | Conscript / light infantry. Cheap, numerous. |
| Monk | Robed healer | 5 | 1 | 1 | 3 | 2 | Support. Heals adjacent allies. Rallies broken units. |

*Values are initial design targets. Final balancing through playtesting.*

### Unit Animations

Each unit has specific animations available from the asset pack:

| Unit | Idle | Run | Attack | Special |
|------|:---:|:---:|:---:|---------|
| Warrior | 8f | 6f | 4f (x2 variants) | Guard (6f) |
| Archer | 6f | 4f | Shoot (8f) | Arrow projectile |
| Lancer | 12f | 6f | 3f (5 directions) | Defence (6f, 5 directions) |
| Pawn | 8f | 6f | -- | Tool variants (axe, knife, hammer, pickaxe) |
| Monk | 6f | 4f | -- | Heal (11f) + Heal Effect (11f) |

**Facing:** Units face right by default. Horizontally mirror sprites for left-facing. The Lancer is unique in having explicit directional attack/defence sprites (Up, UpRight, Right, DownRight, Down), mirrored for the remaining directions.

**Missing animations (design workarounds):**
- **Death** -- Play explosion particle FX (8-10 frames) at unit position, then remove the unit. Optionally fade out over 0.3s.
- **Hit reaction** -- Flash the sprite white or red for 2 frames. Play small explosion FX at impact point.
- **Pawn attack** -- Use tool variant animations (axe swing = `Pawn_Interact Axe`) as melee attack.

### Unit Stats

- **HP (Hit Points)** -- Damage a unit can take before death. Does not regenerate (except via Monk heal).
- **ATK (Attack)** -- Base damage dealt in combat.
- **DEF (Defense)** -- Reduces incoming damage. Damage taken = max(1, attacker ATK - defender DEF + modifiers).
- **MOV (Movement)** -- Number of movement points per turn. Terrain costs deducted per cell.
- **Range** -- Maximum attack distance in cells. 1 = melee only.
- **Morale** -- Hidden stat tracked per unit. Affected by casualties, leadership, and battle state. When morale breaks, the unit flees.

### Unit Abilities

| Unit | Ability | Description |
|------|---------|-------------|
| Warrior | Guard | Enter defensive stance. +2 DEF until next turn. Uses Guard animation. |
| Archer | Volley | Shoot over obstacles (ignores line of sight). Reduced accuracy. |
| Lancer | Charge | Move in a straight line, attack first enemy hit. +1 ATK per cell traveled. Uses directional attack sprites. |
| Pawn | Brace | Hold position. +1 DEF. Cheap unit's survival tool. |
| Monk | Heal | Restore HP to an adjacent ally (heal amount TBD). Uses Heal animation + effect overlay on target. |

## Army System

### Organization

Each army follows a hierarchy:

```
Army (Faction Color)
├── Division A (e.g., Infantry Vanguard)
│   ├── Squad 1: 4 Warriors, 1 Monk
│   ├── Squad 2: 3 Warriors, 2 Pawns
│   └── Squad 3: 3 Pawns, 2 Archers
├── Division B (e.g., Cavalry Wing)
│   ├── Squad 4: 3 Lancers
│   └── Squad 5: 2 Lancers, 2 Archers
├── Division C (e.g., Ranged Support)
│   ├── Squad 6: 4 Archers, 1 Monk
│   └── Squad 7: 4 Archers
└── Reserve
    └── Squad 8: 2 Warriors, 2 Pawns, 1 Monk
```

The player belongs to one squad within one division. Squad composition varies per run.

### Procedural Army Generation

Each army is generated with:

- **Faction** -- One of the 5 color factions (determines visual identity)
- **Size** -- Total soldier count (varies per battle)
- **Composition** -- Mix of unit types (e.g., heavy infantry army = more Warriors, skirmish army = more Archers and Pawns)
- **Quality** -- Average stats modifier (well-trained veterans vs. raw conscripts)
- **Commander personality** -- Affects strategic decisions (see AI Commanders below)

The two armies are generated independently, creating asymmetric matchups. One army might be a small, elite force of Warriors and Lancers facing a large conscript army of Pawns and Archers.

### AI Commanders

Each army has a commander AI (represented by an avatar from the 25 available portraits) that issues orders to divisions. Commanders have personality traits that influence their strategy:

| Personality | Behavior |
|------------|----------|
| Aggressive | Pushes forward quickly, commits reserves early |
| Defensive | Holds strong positions, waits for the enemy to attack |
| Cautious | Probes with skirmishers, commits slowly, preserves forces |
| Flanker | Sends divisions around the sides, tries to surround |

Commanders react to the battle state: if the center is collapsing, they may commit reserves; if a flank is open, they may exploit it.

### Orders

Divisions and squads receive orders from their commander:

| Order | Meaning |
|-------|---------|
| Advance | Move toward the objective or enemy |
| Hold | Maintain current position, engage enemies in range |
| Flank | Move around the enemy's side |
| Retreat | Fall back to a safer position |
| Charge | Rush the enemy (melee units, morale bonus) |
| Support | Move to reinforce another division |

The player sees their squad's current order displayed on the HUD. Following orders may provide morale bonuses to nearby squad members. Disobeying has no direct penalty but your squad fights without you, and isolated soldiers are vulnerable.

## Combat

### Basic Combat

- **Melee** -- Attack an adjacent enemy. Damage = max(1, ATK - target DEF + modifiers). Plays Warrior Attack, Lancer directional Attack, or Pawn tool Interact animation.
- **Ranged** -- Attack an enemy within range and line of sight. Arrow projectile sprite travels from archer to target. Accuracy decreases with distance. Blocked by forest and units.
- **Charge** -- Lancer-specific. Move in a straight line and attack the first enemy hit. Bonus damage based on distance traveled. Uses directional attack sprite matching charge direction.
- **Heal** -- Monk-specific. Restore HP to an adjacent ally. Plays Heal animation on the Monk and Heal_Effect overlay on the target.

### Combat Visual Feedback

Using available particle effects:

| Event | Visual | Asset |
|-------|--------|-------|
| Melee hit | Small explosion at target | `Explosion_01.png` (8f) |
| Ranged hit | Small explosion at target | `Explosion_01.png` (8f) |
| Charge impact | Large explosion at target | `Explosion_02.png` (10f) |
| Unit death | Large explosion, unit fades | `Explosion_02.png` (10f) |
| Movement | Dust puff at feet | `Dust_01.png` (8f) or `Dust_02.png` (10f) |
| Cavalry gallop | Dust trail | `Dust_02.png` (10f) |
| Building on fire | Flame overlay | `Fire_01/02/03.png` |
| Water crossing | Splash | `Water Splash.png` (9f) |
| Heal | Heal effect overlay on target | `Heal_Effect.png` (11f) |

### Modifiers

| Modifier | Effect |
|----------|--------|
| Flanking | +2 ATK when attacking from the side |
| Rear attack | +3 ATK when attacking from behind |
| High ground | +1 ATK for melee, +1 range for ranged |
| Fortified | +2 DEF when in tower/castle |
| In cover | +1 DEF when in village building or forest |
| Surrounded | -1 DEF per adjacent enemy beyond the first |
| Charge momentum | +1 ATK per cell of charge distance (Lancer) |
| Spear brace | Pawn with Brace: bonus damage to charging Lancers |

### Morale

Morale is tracked per unit as a hidden value (0-100). It is affected by:

- **Positive:** Nearby allied units, winning local engagements, following orders, commander nearby, holding fortifications, Monk nearby
- **Negative:** Taking damage, nearby allies dying, outnumbered locally, squad leader killed, army losing overall

When morale hits 0, the unit **breaks** and begins fleeing toward the nearest map edge (uses Run animation). Broken units can be **rallied** by adjacent Monks (morale restored to 30, plays Heal animation). If a critical mass of an army breaks, a **rout** occurs and the battle ends.

### Death

When a unit reaches 0 HP, it dies: an explosion effect plays at its position, the sprite fades out, and the unit is removed from the battlefield. If the player dies, the run ends immediately. A brief death screen shows the player's avatar (from the 25 available portraits), then transitions to the run summary.

## Procedural Generation Details

### What Changes Each Run

| Element | Variation |
|---------|-----------|
| Faction pairing | 10 possible pairings from 5 factions |
| Terrain layout | Different maps with varied terrain distribution |
| Building placement | Different castles, towers, villages, monastery positions |
| Army A composition | Different size, unit mix, quality |
| Army B composition | Different size, unit mix, quality |
| Commander A | Random portrait + personality |
| Commander B | Random portrait + personality |
| Objectives | Different key buildings/positions on the map |
| Player's army | Randomly assigned to one faction |
| Player's squad and role | Different position in the formation, different unit type |

### Generation Constraints

- Both armies should be roughly comparable in total power (within a range, not identical)
- Terrain should not overwhelmingly favor one side's deployment zone
- At least one clear path between the two armies (no impassable walls of water/rocks)
- Each side gets a castle in their deployment zone
- Objectives (towers, monasteries, village clusters) placed in contested areas
- Tilemap color variant selected per run for visual variety (5 options)

## Progression

### Within a Run

There is no leveling or stat growth within a single battle. What you start with is what you have. The only progression within a run is positional: advancing through the battlefield, completing objectives, and surviving. Monks can heal, but HP cannot exceed starting maximum.

### Between Runs (Meta-Progression) -- TBD

The meta-progression system is to be designed. Possible directions include:

- **Unlockable roles** -- Start as Archer, Lancer, Pawn, or Monk instead of Warrior
- **Battle scenarios** -- Unlock specific battle setups (siege, river crossing, ambush)
- **Starting conditions** -- Choose faction, deployment position
- **Cosmetic unlocks** -- Faction preference

The design should preserve the roguelike principle that player skill matters more than accumulated power. No persistent stat upgrades.

## User Interface

The asset pack provides a complete medieval-themed UI kit.

### In-Battle HUD

- **Health bar** -- `BigBar_Base.png` + `BigBar_Fill.png` (stretchable fill, tinted by HP percentage)
- **Morale indicator** -- `SmallBar_Base.png` + `SmallBar_Fill.png` (steady=green, shaken=yellow, breaking=red)
- **Current orders** -- Displayed on a `SmallRibbons.png` banner element
- **Action buttons** -- `SmallBlueSquareButton` (regular/pressed states) with `Icon` overlays for move, attack, wait, ability
- **Minimap** -- Custom rendered overview using faction colors
- **Turn counter** -- Number displayed on `BigRibbons.png`
- **Commander portrait** -- One of 25 `Avatars` showing the current commander issuing orders

### Battle Overview (Toggle)

A zoomed-out view of the entire battlefield showing:

- Both armies' positions in faction colors
- Front line indicator
- Division-level morale bars (`SmallBar`)
- Objective control status (building faction color)
- Commander order indicators

### Menus

Built using the asset pack UI components:

- **Main menu** -- `Banner.png` as title, `BigBlueButton` for New Battle, `BigRedButton` for Settings. `WoodTable.png` as background panel.
- **Pause menu** -- `RegularPaper.png` as dialog background. Resume, Settings, Abandon Run buttons.
- **Settings** -- `SpecialPaper.png` background. Audio volume, controls, display options.
- **Run summary** -- `Banner.png` header with `Swords.png` decoration. Stats on `RegularPaper.png`. Player avatar displayed.

### Cursors

4 cursor variants available (`Cursor_01` through `Cursor_04`). Use different cursors for:
- Default / navigation
- Target selection (attack)
- Move destination
- Invalid action

### Responsive Layout

- **Mobile:** Canvas fills the viewport; UI elements scale to screen size
- **Desktop:** Fixed canvas size with optional fullscreen toggle
- Action buttons must meet minimum touch target size (44x44px per Apple HIG)
- HUD repositioned for mobile: bottom-of-screen action bar instead of top-left overlay
- End Turn button always visible on screen (replaces space bar as primary interaction)

## Input & Controls

Touch is the **primary input method**; mouse and keyboard are supported as secondary/fallback for desktop users.

### Touch Controls (Primary)

| Input | Action | Details |
|-------|--------|---------|
| Swipe anywhere | Move or attack | 8-directional; attacks enemy in range if one exists in swipe direction, otherwise moves (see below) |
| Tap End Turn button | End turn | On-screen button, always visible |
| Pinch (two fingers) | Zoom camera | Replaces mouse wheel zoom |
| Two-finger drag | Pan camera | Replaces WASD/arrow key panning |

### Swipe-Anywhere Movement & Attack (One Action Per Turn)

Each turn allows one action: move OR attack. Swiping anywhere on the screen moves or attacks in the swiped direction. Attack takes priority over movement to avoid accidentally walking past enemies. The turn auto-ends after a completed action.

1. `touchstart` records the starting point (single-touch only; two-finger gestures are reserved for camera)
2. `touchend` computes the delta vector
3. If the swipe distance exceeds a minimum threshold (~30px), the direction is determined from 8 possibilities: **N, NE, E, SE, S, SW, W, NW** (using 45-degree sectors based on swipe angle)
4. **Attack check:** If an attackable enemy exists in the swipe direction (within the same 45-degree sector), the nearest such enemy is attacked. The turn auto-ends.
5. **Movement fallback:** If no enemy is in the swiped direction, the player walks **tile-by-tile in a straight line** until movement points are exhausted or an obstacle is hit.
   - **Enemy interruption:** If an enemy enters attack range during the walk, movement stops immediately and the player may attack before the turn ends. This prevents accidentally walking past threats.
   - **Completed walk:** If the walk finishes without interruption, the turn auto-ends.
6. If no valid action exists in the swiped direction, nothing happens.

This means swiping toward an enemy attacks them, and swiping toward open ground walks as far as possible in that direction. The player never needs to precisely target tiles or units — just indicate a direction.

### Keyboard & Mouse Controls (Secondary)

| Input | Action |
|-------|--------|
| Click tile | Move in direction of tile / attack enemy on tile |
| Mouse wheel | Zoom camera |
| WASD / Arrow keys | Pan camera |
| Space | End turn |

### Visual Style

Top-down chibi pixel art from the Tiny Swords pack. The style is colorful, readable, and charming rather than gritty. Unit identification is clear at a glance thanks to distinct silhouettes per type and strong faction color coding.

**Draw order** (bottom to top):
1. Water background tiles
2. Ground tiles (grass, elevation)
3. Water foam animation
4. Decorations (rocks, bushes, stumps)
5. Building bases
6. Units (Y-sorted: lower units drawn on top)
7. Trees and building tops (units walk behind these)
8. Particle effects (dust, explosions, fire)
9. Projectiles (arrows)
10. UI overlay (HUD, bars, buttons)

## Audio

### Sound Effects

- Sword clashes and weapon impacts (Warrior attacks)
- Arrow release and impact (Archer shoot)
- Lance thrust (Lancer charge)
- Footsteps / movement
- Horse gallop (Lancer movement)
- Healing chime (Monk heal)
- Horns and drums (commander orders changing)
- Ambient crowd noise (distant battle sounds)
- Explosion / impact sounds (matching particle FX)
- Death sounds
- Morale break (panicked shouts)
- Fire crackling (burning buildings)

### Music

- **Main menu** -- Somber, anticipatory medieval theme
- **Battle** -- Dynamic intensity based on proximity to combat. Quiet when in the rear, intense at the front line.
- **Victory/defeat** -- Distinct musical stings for battle outcome
- **Death** -- Brief, impactful musical moment

## Technical Architecture

### Stack

- **Language:** Rust
- **Compilation:** WebAssembly (wasm-pack)
- **Rendering:** HTML Canvas 2D (wasm-bindgen)
- **Offline:** PWA with service worker
- **Deployment:** GitHub Pages via GitHub Actions
- **Touch input:** web-sys TouchEvent, TouchList APIs
- **Mobile scaling:** `<meta name="viewport">` tag for proper mobile rendering
- **Assets:** Tiny Swords (Free Pack) by Pixel Frog

### Sprite Sheet System

All unit animations are horizontal strip sprite sheets. The rendering system needs to:

1. Load PNG sprite sheets as GPU textures
2. Extract frames by index: `source_rect = (frame_index * frame_width, 0, frame_width, frame_height)`
3. Track animation state per unit (current animation, current frame, frame timer)
4. Handle horizontal flipping for left-facing units
5. Handle Lancer's larger frame size (320x320 vs 192x192 for other units)
6. Render particle effects as overlay animations at world positions

### Architecture Principles

The architecture will emerge from the code as needed, following YAGNI. Initial structure:

- **Game state** -- Core data structures representing the battlefield, units, and battle state
- **Systems** -- Logic operating on game state (movement, combat, AI, morale)
- **Rendering** -- HTML Canvas 2D translating game state to visuals, sprite sheet animation
- **Input** -- Player input handling and action mapping
- **Audio** -- Sound playback tied to game events

Whether this evolves into ECS, component-based, or another pattern will be decided based on actual needs during development.

### Performance Targets

- 60 FPS rendering on mid-range hardware
- Turn resolution under 100ms (including all AI actions)
- Initial load under 5MB (WASM + assets)
- Offline-capable after first load

### Asset Loading

The Tiny Swords pack totals ~410 PNG files. Strategy:
- Bundle sprite sheets into texture atlases at build time to reduce draw calls
- Load faction assets on demand (only load the 2 factions used in current battle)
- Cache in service worker for offline play
