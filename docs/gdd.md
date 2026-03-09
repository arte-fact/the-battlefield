# The Battlefield -- Game Design Document

## Game Overview

**Title:** The Battlefield
**Genre:** Roguelike, turn-based tactics
**Platform:** Web (PWA, playable offline)
**Perspective:** Top-down, square grid

**Elevator pitch:** You are one soldier in a massive medieval battle between two armies. Survive. Fight. Turn the tide. Die. Try again.

**Core fantasy:** You're not the general. You're not the hero. You're a soldier in the ranks, following orders, fighting for your life in a battle much larger than yourself. The army clashes around you whether you act brilliantly or not -- but your choices still matter.

## Core Pillars

### 1. One Soldier, One Life

Every run is one soldier's experience of one battle. Permadeath means every decision carries weight. There are no saves, no checkpoints. When you fall, the battle continues without you.

### 2. The Battle Is Bigger Than You

The two armies fight according to their commanders' strategies. Squads advance, hold positions, and retreat based on orders from above. You are part of this machine -- you receive orders, you see the battle unfold around you, but you don't control it. Your actions can influence your local area and sometimes tip the balance, but the outcome depends on the whole army.

### 3. Every Battle Is Different

No two runs are the same. Armies are procedurally generated with different compositions, strengths, and commanders. Terrain varies. Weather changes. The battle you fight today is not the battle you'll fight tomorrow.

## Game Loop

### Run Structure

1. **Battle generation** -- The game generates two opposing armies, a battlefield (terrain, weather, time of day), and battle objectives.
2. **Deployment** -- The player is placed in their squad's starting position within the army formation.
3. **Battle** -- Turn-by-turn gameplay. Each turn, the player receives orders, decides their actions, and the battle progresses.
4. **Resolution** -- The run ends when the player dies (permadeath) or the battle concludes (victory, defeat, or rout of either side).
5. **Summary** -- A post-battle screen shows the player's performance: turns survived, enemies defeated, orders followed, contribution to the battle outcome.
6. **New run** -- Return to battle generation with new procedural conditions.

### Turn Structure

Each turn follows this sequence:

1. **Orders phase** -- The player's squad commander issues or updates orders (advance to position X, hold this line, fall back). Orders are displayed clearly.
2. **Player action phase** -- The player chooses their action: move, attack, use ability, wait, or other context-specific actions.
3. **AI action phase** -- All other units on both sides act simultaneously based on their orders and local AI behavior.
4. **Resolution phase** -- Combat is resolved, morale is updated, units that break begin fleeing, dead units are removed.

## Map and Battlefield

### Grid

The battlefield uses a **square grid**. Each cell can contain terrain features and at most one unit.

### Terrain Types

| Terrain | Movement cost | Defense bonus | Notes |
|---------|:---:|:---:|-------|
| Open field | 1 | 0 | Default terrain |
| Hill | 2 | +1 | Height advantage for ranged attacks |
| Forest | 2 | +1 | Blocks line of sight |
| River | 3 | -1 | Slows movement, defensive penalty |
| Bridge | 1 | 0 | Crossing point over rivers |
| Fortification | 1 | +2 | Walls, barricades, defensive structures |
| Village | 1 | +1 | Buildings, partial cover |
| Mud | 2 | 0 | Created by rain on open fields |

### Weather

Weather is determined at battle generation and affects the entire battlefield.

| Weather | Effect |
|---------|--------|
| Clear | No modifiers |
| Rain | Open fields become mud. Reduced visibility range. |
| Fog | Heavily reduced visibility range. |
| Wind | Affects ranged attack accuracy at distance. |

### Battlefield Zones

The battlefield is conceptually divided into zones that inform army AI behavior:

- **Frontline** -- Where the two armies meet. Most combat happens here.
- **Flanks** -- The sides of the engagement. Flanking maneuvers target these.
- **Rear** -- Behind the front. Commanders, reserves, and archers.
- **Objectives** -- Key positions on the map (hilltops, bridges, villages) that commanders order units to capture or defend.

### Procedural Generation

Terrain is generated with constraints to ensure playable, interesting battlefields:

- A mostly open center where the main engagement happens
- Terrain features distributed to create tactical variety (flanking routes through forests, defensible hills, river crossings as chokepoints)
- Objectives placed at strategically interesting locations
- Both sides have reasonable deployment zones

## Units and Roles

### Player Character

The player starts each run as a soldier. The starting role determines their stats and available actions.

**Starting role (default):** Swordsman (infantry)

Additional roles may be unlocked through the meta-progression system (TBD).

### Unit Types

| Unit | HP | ATK | DEF | MOV | Range | Notes |
|------|:---:|:---:|:---:|:---:|:---:|-------|
| Swordsman | 10 | 3 | 2 | 3 | 1 | Balanced melee infantry |
| Spearman | 8 | 2 | 3 | 3 | 1 | Defensive, bonus vs cavalry |
| Archer | 6 | 2 | 1 | 3 | 5 | Ranged attacks, weak in melee |
| Cavalry | 10 | 4 | 1 | 5 | 1 | Fast, powerful charge, weak when surrounded |
| Shield bearer | 12 | 1 | 4 | 2 | 1 | High defense, protects adjacent allies |

*Values are initial design targets. Final balancing through playtesting.*

### Unit Stats

- **HP (Hit Points)** -- Damage a unit can take before death. Does not regenerate.
- **ATK (Attack)** -- Base damage dealt in combat.
- **DEF (Defense)** -- Reduces incoming damage. Damage taken = max(1, attacker ATK - defender DEF + modifiers).
- **MOV (Movement)** -- Number of movement points per turn. Terrain costs deducted per cell.
- **Range** -- Maximum attack distance in cells. 1 = melee only.
- **Morale** -- Hidden stat tracked per unit. Affected by casualties, leadership, and battle state. When morale breaks, the unit flees.

## Army System

### Organization

Each army follows a hierarchy:

```
Army
├── Division A
│   ├── Squad 1 (8-12 soldiers)
│   ├── Squad 2
│   └── Squad 3
├── Division B
│   ├── Squad 4
│   ├── Squad 5
│   └── Squad 6
└── Reserve
    └── Squad 7
```

The player belongs to one squad within one division.

### Procedural Army Generation

Each army is generated with:

- **Size** -- Total soldier count (varies per battle)
- **Composition** -- Mix of unit types (e.g., heavy infantry army vs. archer-heavy army)
- **Quality** -- Average stats modifier (well-trained veterans vs. raw conscripts)
- **Commander personality** -- Affects strategic decisions (see AI Commanders below)

The two armies are generated independently, creating asymmetric matchups.

### AI Commanders

Each army has a commander AI that issues orders to divisions. Commanders have personality traits that influence their strategy:

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

- **Melee** -- Attack an adjacent enemy. Damage = max(1, ATK - target DEF + modifiers).
- **Ranged** -- Attack an enemy within range and line of sight. Accuracy decreases with distance. Blocked by forest and units.
- **Charge** -- Cavalry-specific. Move in a straight line and attack the first enemy hit. Bonus damage based on distance traveled. Cannot charge through obstacles.

### Modifiers

| Modifier | Effect |
|----------|--------|
| Flanking | +2 ATK when attacking from the side |
| Rear attack | +3 ATK when attacking from behind |
| High ground | +1 ATK for melee, +1 range for ranged |
| Fortified | +2 DEF when in fortification |
| Surrounded | -1 DEF per adjacent enemy beyond the first |
| Charge momentum | +1 ATK per cell of charge distance |

### Morale

Morale is tracked per unit as a hidden value (0-100). It is affected by:

- **Positive:** Nearby allied units, winning local engagements, following orders, commander nearby, holding fortifications
- **Negative:** Taking damage, nearby allies dying, outnumbered locally, squad leader killed, army losing overall

When morale hits 0, the unit **breaks** and begins fleeing toward the nearest map edge. Broken units can be **rallied** by adjacent commanders or squad leaders (morale restored to 30). If a critical mass of an army breaks, a **rout** occurs and the battle ends.

### Death

When a unit reaches 0 HP, it dies and is removed from the battlefield. If the player dies, the run ends immediately. A brief death screen shows the killing blow, then transitions to the run summary.

## Procedural Generation Details

### What Changes Each Run

| Element | Variation |
|---------|-----------|
| Terrain layout | Different maps with varied terrain distribution |
| Weather | Clear, rain, fog, or wind |
| Time of day | Dawn, midday, dusk (affects visibility) |
| Army A composition | Different size, unit mix, quality |
| Army B composition | Different size, unit mix, quality |
| Commander A personality | Aggressive, defensive, cautious, flanker |
| Commander B personality | Aggressive, defensive, cautious, flanker |
| Objectives | Different key positions on the map |
| Player's army | Randomly assigned to Army A or B |
| Player's squad position | Different position in the formation |

### Generation Constraints

- Both armies should be roughly comparable in total power (within a range, not identical)
- Terrain should not overwhelmingly favor one side's deployment zone
- At least one clear path between the two armies (no impassable walls of terrain)
- Objectives should be reachable by both sides

## Progression

### Within a Run

There is no leveling or stat growth within a single battle. What you start with is what you have. The only progression within a run is positional: advancing through the battlefield, completing objectives, and surviving.

### Between Runs (Meta-Progression) -- TBD

The meta-progression system is to be designed. Possible directions include:

- **Unlockable roles** -- Start as different unit types (archer, cavalry, etc.)
- **Battle scenarios** -- Unlock specific battle setups (siege, ambush, river crossing)
- **Starting conditions** -- Choose deployment position, squad composition
- **Cosmetic unlocks** -- Visual customization

The design should preserve the roguelike principle that player skill matters more than accumulated power. No persistent stat upgrades.

## User Interface

### In-Battle HUD

- **Health bar** -- Player's current HP
- **Morale indicator** -- Player's morale state (steady, shaken, breaking)
- **Current orders** -- The squad's current order, displayed prominently
- **Action buttons** -- Move, attack, wait, ability (context-dependent)
- **Minimap** -- Overview of the battlefield showing army positions, front line, and objectives
- **Turn counter** -- Current turn number

### Battle Overview (Toggle)

A zoomed-out view of the entire battlefield showing:

- Both armies' positions as colored blocks
- Front line indicator
- Division-level morale bars
- Objective control status
- Commander order indicators

### Menus

- **Main menu** -- New battle, settings, credits
- **Pause menu** -- Resume, settings, abandon run
- **Settings** -- Audio volume, controls, display options
- **Run summary** -- Post-battle statistics and performance

### Visual Style

Top-down view with animated sprite tiles from an itch.io asset pack. The art style should feel grounded and readable -- clear unit identification at a glance, visible terrain differences, and distinct army colors.

## Audio

### Sound Effects

- Sword clashes and weapon impacts (per combat action)
- Footsteps on different terrain
- Arrow volleys
- Cavalry charges
- Horns and drums (commander orders)
- Ambient crowd noise (distant battle sounds)
- Death sounds
- Morale break (panicked shouts)

### Music

- **Main menu** -- Somber, anticipatory medieval theme
- **Battle** -- Dynamic intensity based on proximity to combat. Quiet when in the rear, intense at the front line.
- **Victory/defeat** -- Distinct musical stings for battle outcome
- **Death** -- Brief, impactful musical moment

## Technical Architecture

### Stack

- **Language:** Rust
- **Compilation:** WebAssembly (wasm-pack)
- **Rendering:** WebGPU (wgpu crate)
- **Offline:** PWA with service worker
- **Deployment:** GitHub Pages via GitHub Actions
- **Assets:** Sprite sheets from itch.io asset pack

### Architecture Principles

The architecture will emerge from the code as needed, following YAGNI. Initial structure:

- **Game state** -- Core data structures representing the battlefield, units, and battle state
- **Systems** -- Logic operating on game state (movement, combat, AI, morale)
- **Rendering** -- WebGPU pipeline translating game state to visuals
- **Input** -- Player input handling and action mapping
- **Audio** -- Sound playback tied to game events

Whether this evolves into ECS, component-based, or another pattern will be decided based on actual needs during development.

### Performance Targets

- 60 FPS rendering on mid-range hardware
- Turn resolution under 100ms (including all AI actions)
- Initial load under 5MB (WASM + assets)
- Offline-capable after first load
