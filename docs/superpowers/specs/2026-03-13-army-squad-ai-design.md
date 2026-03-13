# Army, Squad AI & Unit Abilities — Vertical Slice Design

**Date:** 2026-03-13
**Approach:** Vertical slice (Approach B) — thinnest playable version of all systems, then iterate.

## Goal

Replace the current hardcoded demo battle and dumb AI (each unit walks toward nearest enemy) with:

1. Procedurally generated two-faction armies organized into squads
2. Squad-level AI that follows commander orders (Advance / Hold)
3. Lancer Charge and Monk Heal abilities

The player experience changes from "fighting a disorganized mob" to "being one soldier in a structured battle where squads advance together."

## Architecture Overview

```
Seed
 ├─ generate_battlefield(seed) → Grid           [existing]
 └─ generate_army(faction, seed, side) → Army    [new]
      └─ Squad[]
           └─ unit_ids[] → Unit (with squad_id)

Per turn:
  Player acts (unchanged)
  Commander updates orders (trivial for now: all Advance)
  For each squad (player's squad first):
    Squad AI executes order → units move/attack/charge/heal
    TurnEvents recorded → TurnAnimator plays back
```

## Data Model

### New file: `src/army.rs`

```rust
pub type SquadId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Advance,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Personality {
    Aggressive,
}

pub struct Commander {
    pub personality: Personality,
    pub portrait_index: u8,  // 0-24, from the 25 avatars
}

pub struct Squad {
    pub id: SquadId,
    pub faction: Faction,
    pub unit_ids: Vec<UnitId>,
    pub order: Order,
    pub target: Option<(u32, u32)>,   // grid position to advance toward / hold at
    pub leader_id: Option<UnitId>,     // first surviving unit is leader
}

pub struct Army {
    pub faction: Faction,
    pub commander: Commander,
    pub squads: Vec<Squad>,
}
```

### Unit changes (`src/unit.rs`)

Add field to `Unit`:
```rust
pub squad_id: Option<SquadId>,
```

Initialized to `None` in `Unit::new()`. Add `squad_id: Option<SquadId>` as a new parameter to `Game::spawn_unit()` so the army generator can set it at spawn time (avoids a post-spawn lookup).

### Game changes (`src/game.rs`)

Add field to `Game`:
```rust
pub armies: Vec<Army>,
```

Replace `setup_demo_battle` / `setup_demo_battle_with_seed` with:
```rust
pub fn setup_battle(&mut self, seed: u32)
```

Add helper:
```rust
pub fn squad_for_unit(&self, unit_id: UnitId) -> Option<&Squad>
```

## Procedural Army Generation

### Function signature

```rust
pub fn generate_army(
    faction: Faction,
    rng: &mut LcgRng,
    spawn_side: SpawnSide,
    grid: &Grid,
) -> (Army, Vec<UnitSpec>)
```

`UnitSpec` is a lightweight struct used to feed into `game.spawn_unit()`:
```rust
pub struct UnitSpec {
    pub kind: UnitKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
    pub is_player: bool,
    pub squad_id: SquadId,
}
```

### Squad role templates

| Squad Role | Composition | Placement |
|---|---|---|
| Infantry | 3 Warriors + 1 Pawn | Front row |
| Skirmisher | 2 Pawns + 2 Archers | Front/flank |
| Ranged | 3 Archers + 1 Monk | Behind infantry |
| Cavalry | 2 Lancers + 1 Pawn | Flanks |

Each army gets 3-4 squads, chosen by weighted random from the roles above. Both armies always get at least one Infantry squad.

### Spatial placement

- Blue spawns in west margin (x: 2..SPAWN_MARGIN-2), Red in east margin (x: GRID_SIZE-SPAWN_MARGIN+2..GRID_SIZE-2), where SPAWN_MARGIN=12
- Squads placed in vertical formation bands:
  - Infantry/Skirmisher: closest to center (higher x for Blue, lower x for Red)
  - Ranged: behind infantry
  - Cavalry: offset vertically (flanks)
- Units within a squad spawn within 2-3 tiles of each other
- Generator checks `grid.is_passable()` and nudges units to nearest valid tile
- Player assigned to a random Infantry or Skirmisher squad (frontline)

### Commander

- Random portrait index (0-24)
- Personality: Aggressive (all squads get `Order::Advance`)
- Same seed makes same army — deterministic via `LcgRng`

### Spawn side

```rust
pub enum SpawnSide { West, East }
```

Determines x-range for placement and initial facing direction.

## Squad AI

### Replaces current `ai_turn()`

The current `ai_turn` iterates all non-player units individually. The new version iterates squads.

### Algorithm

```
fn ai_turn(&mut self, position_snapshot):
    // Collect squad action order: player's squad first, then others
    let squad_order = player_squad_first(self.armies)

    for squad in squad_order:
        let alive_units = squad.unit_ids filtered to alive, excluding player
        if alive_units.is_empty(): continue

        let squad_centroid = average position of alive units in squad

        match squad.order:
            Advance:
                squad.target = centroid of nearest enemy squad
                for unit in alive_units:
                    unit_action(unit):  // see unit action dispatch below
                    else: move_toward_target(unit, squad.target, squad_centroid)
            Hold:
                // target frozen when order was issued
                for unit in alive_units:
                    unit_action(unit):  // same dispatch
                    else if distance(unit, squad.target) > 3: move toward target
                    else: stay put

    // Orphaned units (dead squad or no squad): fallback nearest-enemy behavior

// Unit action dispatch (per unit type):
fn unit_action(unit, squad_target, squad_centroid):
    match unit.kind:
        Monk:
            if adjacent_ally_below_60_pct(unit): heal lowest-HP adjacent ally
            else: move_toward_target(unit, squad_target, squad_centroid)
        Lancer:
            if can_charge(unit): execute_charge
            else if can_melee(unit, enemy): melee attack
            else: move_toward_target(unit, squad_target, squad_centroid)
        Archer:
            if can_ranged_attack(unit, enemy): ranged attack (using position snapshot)
            else if can_melee(unit, enemy): melee attack
            else: move_toward_target(unit, squad_target, squad_centroid)
        Warrior | Pawn:
            if can_melee(unit, enemy): melee attack
            else: move_toward_target(unit, squad_target, squad_centroid)
```

**Key:** Monks are explicitly dispatched to heal-or-move only. They never enter the attack branch. This prevents `can_ranged_attack` (which returns true for Monks due to `range: 2`) from being called.

### Movement scoring

When choosing which adjacent tile to step to:
```
score(tile) = -3 * distance(tile, squad_target)   // primary: move toward objective
              - 1 * distance(tile, squad_centroid) // secondary: stay with squad
```

Pick the passable, unoccupied tile with the highest score. This naturally keeps squads cohesive while advancing.

### Squad cohesion

Units prefer tiles within 4 tiles of squad centroid. If a unit would move beyond 4 tiles from centroid, it deprioritizes that move (still possible if no better option).

### Turn order within squads

1. Melee units act first (Warriors, Lancers, Pawns)
2. Then ranged (Archers)
3. Then support (Monks)

This prevents Monks from walking into melee range before Warriors engage.

### Turn order: player and squad mates

1. `player_step()` executes the player's action (unchanged)
2. `player_step()` calls `ai_turn()` which processes all squads
3. Player's squad is processed first in `ai_turn()` — the player's squad mates act immediately after the player
4. Then remaining squads act in army order (player's army, then enemy army)

### Squad dissolution

If all units in a squad are dead, the squad is removed from the army. Remaining units that somehow reference it fall back to nearest-enemy behavior.

## Unit Abilities

### Lancer Charge

**Trigger:** A Lancer's nearest enemy is 2+ tiles away in a straight line (cardinal or diagonal) with no blocking terrain or friendly units in between.

**Mechanic:**
- Lancer moves along the straight line, stopping adjacent to the first enemy hit
- Damage = base ATK + 1 per tile traveled
- `can_charge(attacker, target, grid, units) -> Option<Vec<(u32,u32)>>` returns the path if valid

**Charge path rules:**
- The path must be a straight line in one of 8 directions (cardinal or 45-degree diagonal)
- Every tile along the path must be passable terrain and not occupied by any unit (friendly or enemy)
- If an enemy is encountered along the line before the intended target, the Lancer stops adjacent to that closer enemy instead
- `can_charge` scans tiles along the direction from the Lancer; returns `None` if terrain or a friendly unit blocks the path before reaching any enemy

**New combat functions:**
```rust
pub fn calc_charge_damage(attacker: &Unit, defender: &Unit, grid: &Grid, distance: u32) -> i32 {
    let terrain_def = grid.get(defender.grid_x, defender.grid_y).defense_bonus();
    let elev_def = grid.elevation_defense_bonus(defender.grid_x, defender.grid_y);
    (attacker.stats.atk + distance as i32 - defender.stats.def - terrain_def - elev_def).max(1)
}

/// Execute a charge: move the Lancer along the path, apply damage, set flags.
/// Uses the same split_at_mut pattern as execute_attack in game.rs.
pub fn execute_charge(attacker: &mut Unit, defender: &mut Unit, grid: &Grid, path: &[(u32, u32)]) -> CombatResult {
    let distance = path.len() as u32;
    // Move lancer to last tile in path (adjacent to defender)
    if let Some(&(dest_x, dest_y)) = path.last() {
        attacker.grid_x = dest_x;
        attacker.grid_y = dest_y;
    }
    let damage = calc_charge_damage(attacker, defender, grid, distance);
    defender.take_damage(damage);
    attacker.has_attacked = true;
    attacker.has_moved = true;
    // Face the defender
    if defender.grid_x > attacker.grid_x {
        attacker.facing = Facing::Right;
    } else if defender.grid_x < attacker.grid_x {
        attacker.facing = Facing::Left;
    }
    attacker.set_anim(UnitAnim::Attack);
    CombatResult { damage, target_killed: !defender.alive }
}
```

**AI behavior:** Lancers check for charge opportunities before normal movement. Prefer charge over regular move when available.

**Animation:** New `TurnEvent::Charge { unit_id, path, target_id, damage, killed }`. Animator plays fast lerp along path (2x normal speed) + attack impact at end.

### Monk Heal

**Trigger:** A Monk has an adjacent ally below 60% HP.

**Mechanic:**
- Heals lowest-HP adjacent ally for 3 HP (capped at max_hp)
- Consumes the Monk's action (no attack or move)
- Uses existing `UnitAnim::Attack` (Monk's attack frames = heal animation)

**New combat function:**
```rust
pub fn execute_heal(healer: &mut Unit, target: &mut Unit) -> i32 {
    let heal_amount = 3.min(target.stats.max_hp - target.hp);
    target.hp += heal_amount;
    healer.has_attacked = true;
    healer.set_anim(UnitAnim::Attack);
    heal_amount
}
```

**AI behavior:** Monks heal if any adjacent ally is below 60% HP. Otherwise follow squad movement. Monks never attack (melee or ranged) — they are excluded from the attack branch in the unit action dispatch (see Squad AI section). Their `range: 2` stat is reserved for future use (e.g., ranged heal at distance).

**Animation:** New `TurnEvent::Heal { healer_id, target_id, amount }`. Animator plays heal animation on monk + could spawn a heal particle on target (using existing particle system).

## Animation Integration

### New TurnEvent variants

```rust
TurnEvent::Charge {
    unit_id: UnitId,
    path: Vec<(u32, u32)>,
    target_id: UnitId,
    damage: i32,
    killed: bool,
}

TurnEvent::Heal {
    healer_id: UnitId,
    target_id: UnitId,
    amount: i32,
}
```

### New AnimPhase handlers in TurnAnimator

- **Charge:** fast lerp along path tiles (2x speed of normal `ParallelMoves`), then attack impact (explosion particle + damage)
- **Heal:** heal animation on monk (existing attack anim slot), green particle or flash on target

## Module Layout

| File | Changes |
|---|---|
| `src/army.rs` | **New.** Army, Squad, Commander, Order, Personality, UnitSpec, SpawnSide, generate_army, squad AI helpers |
| `src/unit.rs` | Add `squad_id: Option<SquadId>` field |
| `src/game.rs` | Add `armies` field, `setup_battle(seed)`, rewrite `ai_turn()`, add `squad_for_unit()` |
| `src/combat.rs` | Add `calc_charge_damage`, `execute_charge`, `execute_heal`, `can_charge`. Note: `execute_heal` and `execute_charge` take `&mut Unit` pairs — callers in `game.rs` must use the same `split_at_mut` borrow pattern already used in `Game::execute_attack` |
| `src/animation.rs` | Add `Charge` and `Heal` TurnEvent variants + AnimPhase handlers |
| `src/mapgen/mod.rs` | Export `LcgRng` as `pub struct` with `pub fn new()` and `pub fn next()` (army gen needs it) |

## Out of Scope

- Divisions (squads report directly to commander)
- Commander personality variety (just Aggressive)
- Morale system
- Player ability selection UI
- Warrior Guard, Pawn Brace, Archer Volley abilities
- Buildings as terrain
- HUD showing squad orders
- Commander portrait rendering
