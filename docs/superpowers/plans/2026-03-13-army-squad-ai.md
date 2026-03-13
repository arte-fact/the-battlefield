# Army, Squad AI & Unit Abilities Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hardcoded demo battle with procedurally generated two-faction armies organized into squads, squad-level AI that follows orders, and Lancer Charge + Monk Heal abilities.

**Architecture:** New `army.rs` module contains data types (Army, Squad, Commander, Order) and army generation. Unit gets `squad_id` field. `game.rs` gets `armies` field and rewritten `ai_turn()` that iterates squads with per-unit-type action dispatch. `combat.rs` gets charge/heal functions. `animation.rs` gets Charge/Heal event variants.

**Tech Stack:** Rust, wasm-bindgen, Canvas 2D

**Spec:** `docs/superpowers/specs/2026-03-13-army-squad-ai-design.md`

---

## Chunk 1: Data Model + Unit Changes

### Task 1: Export LcgRng from mapgen

**Files:**
- Modify: `src/mapgen/mod.rs:174-185`

- [ ] **Step 1: Make LcgRng and its methods pub**

In `src/mapgen/mod.rs`, change:

```rust
pub struct LcgRng(pub u64);

impl LcgRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}
```

- [ ] **Step 2: Run tests to verify nothing breaks**

Run: `cargo test 2>&1 | tail -3`
Expected: all existing tests pass

- [ ] **Step 3: Commit**

```bash
git add src/mapgen/mod.rs
git commit -m "refactor: export LcgRng as pub from mapgen"
```

### Task 2: Add squad_id to Unit

**Files:**
- Modify: `src/unit.rs:140-198` (Unit struct + new())

- [ ] **Step 1: Write failing test for squad_id field**

In `src/unit.rs` tests section (after line 318), add:

```rust
#[test]
fn unit_squad_id_defaults_to_none() {
    let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, false);
    assert_eq!(unit.squad_id, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test unit::tests::unit_squad_id_defaults_to_none 2>&1 | tail -5`
Expected: FAIL — `squad_id` field doesn't exist

- [ ] **Step 3: Add squad_id field to Unit struct and initialize in new()**

In `src/unit.rs`, add to `Unit` struct (after `visual_y` field, line 164):

```rust
/// Squad this unit belongs to (None for orphaned units).
pub squad_id: Option<u32>,
```

In `Unit::new()`, add to the Self block (after `visual_y: vy,` line 196):

```rust
squad_id: None,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test 2>&1 | tail -3`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/unit.rs
git commit -m "feat: add squad_id field to Unit"
```

### Task 3: Create army.rs with data types

**Files:**
- Create: `src/army.rs`
- Modify: `src/lib.rs:1-12` (add module declaration)

- [ ] **Step 1: Write army.rs with data types and basic tests**

Create `src/army.rs`:

```rust
use crate::unit::{Faction, UnitId, UnitKind};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnSide {
    West,
    East,
}

pub struct Commander {
    pub personality: Personality,
    pub portrait_index: u8,
}

pub struct Squad {
    pub id: SquadId,
    pub faction: Faction,
    pub unit_ids: Vec<UnitId>,
    pub order: Order,
    /// Grid position this squad is advancing toward or holding.
    pub target: Option<(u32, u32)>,
    /// First surviving unit is leader.
    pub leader_id: Option<UnitId>,
}

pub struct Army {
    pub faction: Faction,
    pub commander: Commander,
    pub squads: Vec<Squad>,
}

/// Lightweight spec for spawning a unit during army generation.
pub struct UnitSpec {
    pub kind: UnitKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
    pub is_player: bool,
    pub squad_id: SquadId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SquadRole {
    Infantry,
    Skirmisher,
    Ranged,
    Cavalry,
}

impl SquadRole {
    /// Returns the unit composition for this squad role.
    pub fn composition(self) -> Vec<UnitKind> {
        match self {
            SquadRole::Infantry => vec![
                UnitKind::Warrior,
                UnitKind::Warrior,
                UnitKind::Warrior,
                UnitKind::Pawn,
            ],
            SquadRole::Skirmisher => vec![
                UnitKind::Pawn,
                UnitKind::Pawn,
                UnitKind::Archer,
                UnitKind::Archer,
            ],
            SquadRole::Ranged => vec![
                UnitKind::Archer,
                UnitKind::Archer,
                UnitKind::Archer,
                UnitKind::Monk,
            ],
            SquadRole::Cavalry => vec![
                UnitKind::Lancer,
                UnitKind::Lancer,
                UnitKind::Pawn,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squad_role_composition_sizes() {
        assert_eq!(SquadRole::Infantry.composition().len(), 4);
        assert_eq!(SquadRole::Skirmisher.composition().len(), 4);
        assert_eq!(SquadRole::Ranged.composition().len(), 4);
        assert_eq!(SquadRole::Cavalry.composition().len(), 3);
    }

    #[test]
    fn infantry_has_warriors() {
        let comp = SquadRole::Infantry.composition();
        let warriors = comp.iter().filter(|&&k| k == UnitKind::Warrior).count();
        assert_eq!(warriors, 3);
    }

    #[test]
    fn order_default_advance() {
        let order = Order::Advance;
        assert_eq!(order, Order::Advance);
    }
}
```

- [ ] **Step 2: Add module declaration to lib.rs**

In `src/lib.rs`, add after `pub mod animation;` (line 1):

```rust
pub mod army;
```

- [ ] **Step 3: Run tests**

Run: `cargo test army::tests 2>&1 | tail -5`
Expected: 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/army.rs src/lib.rs
git commit -m "feat: add army.rs with data types and squad roles"
```

### Task 4: Add armies field to Game and update spawn_unit

**Files:**
- Modify: `src/game.rs:1-80` (Game struct, new(), spawn_unit())

- [ ] **Step 1: Write test for spawn_unit with squad_id**

In `src/game.rs` tests section (after the existing `spawned_unit_has_correct_visual_position` test, around line 1057), add:

```rust
#[test]
fn spawn_unit_with_squad_id() {
    let mut game = Game::new(960.0, 640.0);
    let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true, Some(1));
    let unit = game.find_unit(id).unwrap();
    assert_eq!(unit.squad_id, Some(1));
}

#[test]
fn spawn_unit_without_squad_id() {
    let mut game = Game::new(960.0, 640.0);
    let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true, None);
    let unit = game.find_unit(id).unwrap();
    assert_eq!(unit.squad_id, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test game::tests::spawn_unit_with_squad 2>&1 | tail -5`
Expected: FAIL — `spawn_unit` takes wrong number of args

- [ ] **Step 3: Add squad_id parameter to spawn_unit, add armies field to Game**

In `src/game.rs`, add import at top (after line 1):

```rust
use crate::army::Army;
```

Add `armies` field to `Game` struct (after `turn_events` field, line 36):

```rust
/// The two armies in the current battle.
pub armies: Vec<Army>,
```

Initialize in `Game::new()` (after `turn_events: Vec::new(),` line 63):

```rust
armies: Vec::new(),
```

Update `spawn_unit` signature (line 67-80) to accept `squad_id`:

```rust
pub fn spawn_unit(
    &mut self,
    kind: UnitKind,
    faction: Faction,
    x: u32,
    y: u32,
    is_player: bool,
    squad_id: Option<u32>,
) -> UnitId {
    let id = self.next_unit_id;
    self.next_unit_id += 1;
    let mut unit = Unit::new(id, kind, faction, x, y, is_player);
    unit.squad_id = squad_id;
    self.units.push(unit);
    id
}
```

- [ ] **Step 4: Update ALL existing spawn_unit call sites to pass None**

Every existing `self.spawn_unit(...)` and `game.spawn_unit(...)` call needs `None` appended as the last argument. This includes:
- `setup_demo_battle_with_seed` in `src/game.rs` (~lines 715-818, about 18 calls)
- All test calls in `src/game.rs` (~lines 840-1064, about 20+ calls)

Search for `spawn_unit(` in `src/game.rs` and append `, None` before the closing `)` for each call.

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass including the two new ones

- [ ] **Step 6: Commit**

```bash
git add src/game.rs
git commit -m "feat: add armies field to Game, squad_id param to spawn_unit"
```

### Task 5: Add squad_for_unit helper

**Files:**
- Modify: `src/game.rs` (add method to Game impl)

- [ ] **Step 1: Write test**

In `src/game.rs` tests, add:

```rust
#[test]
fn squad_for_unit_returns_squad() {
    use crate::army::{Army, Commander, Order, Personality, Squad};
    let mut game = Game::new(960.0, 640.0);
    let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true, Some(1));
    game.armies.push(Army {
        faction: Faction::Blue,
        commander: Commander {
            personality: Personality::Aggressive,
            portrait_index: 0,
        },
        squads: vec![Squad {
            id: 1,
            faction: Faction::Blue,
            unit_ids: vec![id],
            order: Order::Advance,
            target: None,
            leader_id: Some(id),
        }],
    });
    let squad = game.squad_for_unit(id);
    assert!(squad.is_some());
    assert_eq!(squad.unwrap().id, 1);
}

#[test]
fn squad_for_unit_returns_none_for_orphan() {
    let mut game = Game::new(960.0, 640.0);
    let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true, None);
    assert!(game.squad_for_unit(id).is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test game::tests::squad_for_unit 2>&1 | tail -5`
Expected: FAIL — method doesn't exist

- [ ] **Step 3: Implement squad_for_unit**

Add to `Game` impl (after `find_unit` method, around line 98):

```rust
pub fn squad_for_unit(&self, unit_id: UnitId) -> Option<&crate::army::Squad> {
    self.armies
        .iter()
        .flat_map(|a| &a.squads)
        .find(|s| s.unit_ids.contains(&unit_id))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test game::tests::squad_for_unit 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/game.rs
git commit -m "feat: add squad_for_unit helper to Game"
```

---

## Chunk 2: Army Generation

### Task 6: Implement generate_army

**Files:**
- Modify: `src/army.rs` (add generate_army function)

- [ ] **Step 1: Write tests for army generation**

Add to `src/army.rs` tests:

```rust
use crate::grid::{Grid, GRID_SIZE};
use crate::mapgen::LcgRng;

#[test]
fn generate_army_produces_squads() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng = LcgRng::new(42);
    let (army, specs) = generate_army(Faction::Blue, &mut rng, SpawnSide::West, &grid, false);
    assert_eq!(army.faction, Faction::Blue);
    assert!(army.squads.len() >= 3);
    assert!(army.squads.len() <= 4);
    assert!(!specs.is_empty());
    // At least one infantry squad
    assert!(army.squads.iter().any(|s| s.unit_ids.len() == 4));
}

#[test]
fn generate_army_deterministic() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng1 = LcgRng::new(42);
    let mut rng2 = LcgRng::new(42);
    let (_, specs1) = generate_army(Faction::Blue, &mut rng1, SpawnSide::West, &grid, false);
    let (_, specs2) = generate_army(Faction::Blue, &mut rng2, SpawnSide::West, &grid, false);
    assert_eq!(specs1.len(), specs2.len());
    for (a, b) in specs1.iter().zip(specs2.iter()) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.grid_x, b.grid_x);
        assert_eq!(a.grid_y, b.grid_y);
    }
}

#[test]
fn generate_army_west_spawns_in_west() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng = LcgRng::new(42);
    let (_, specs) = generate_army(Faction::Blue, &mut rng, SpawnSide::West, &grid, false);
    for spec in &specs {
        assert!(spec.grid_x < 12, "West army unit at x={}", spec.grid_x);
    }
}

#[test]
fn generate_army_east_spawns_in_east() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng = LcgRng::new(42);
    let (_, specs) = generate_army(Faction::Red, &mut rng, SpawnSide::East, &grid, false);
    for spec in &specs {
        assert!(spec.grid_x >= GRID_SIZE - 12, "East army unit at x={}", spec.grid_x);
    }
}

#[test]
fn generate_army_player_assigned_to_frontline() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng = LcgRng::new(42);
    let (army, specs) = generate_army(Faction::Blue, &mut rng, SpawnSide::West, &grid, true);
    let player_spec = specs.iter().find(|s| s.is_player);
    assert!(player_spec.is_some(), "Player should be in the army");
    let player_squad_id = player_spec.unwrap().squad_id;
    // Player should be in a squad
    assert!(army.squads.iter().any(|s| s.id == player_squad_id));
}

#[test]
fn generate_army_all_units_on_passable_tiles() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng = LcgRng::new(42);
    let (_, specs) = generate_army(Faction::Blue, &mut rng, SpawnSide::West, &grid, false);
    for spec in &specs {
        assert!(
            grid.is_passable(spec.grid_x, spec.grid_y),
            "Unit at ({},{}) on impassable tile",
            spec.grid_x,
            spec.grid_y
        );
    }
}

#[test]
fn generate_army_no_overlapping_positions() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut rng = LcgRng::new(42);
    let (_, specs) = generate_army(Faction::Blue, &mut rng, SpawnSide::West, &grid, false);
    let mut positions: Vec<(u32, u32)> = specs.iter().map(|s| (s.grid_x, s.grid_y)).collect();
    positions.sort();
    positions.dedup();
    assert_eq!(positions.len(), specs.len(), "Units have overlapping positions");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test army::tests::generate_army 2>&1 | tail -5`
Expected: FAIL — `generate_army` doesn't exist

- [ ] **Step 3: Implement generate_army**

Add imports at top of `src/army.rs`:

```rust
use crate::grid::{Grid, GRID_SIZE};
use crate::mapgen::LcgRng;
```

Add constant:

```rust
const SPAWN_MARGIN: u32 = 12;
```

Add the function after the `SquadRole` impl block:

```rust
/// Generate an army for one faction.
/// If `has_player` is true, one unit in a frontline squad is marked as player.
pub fn generate_army(
    faction: Faction,
    rng: &mut LcgRng,
    spawn_side: SpawnSide,
    grid: &Grid,
    has_player: bool,
) -> (Army, Vec<UnitSpec>) {
    // Pick 3-4 squad roles, always starting with Infantry
    let num_squads = 3 + (rng.next() % 2) as usize; // 3 or 4
    let extra_roles = [SquadRole::Skirmisher, SquadRole::Ranged, SquadRole::Cavalry, SquadRole::Infantry];
    let mut roles = vec![SquadRole::Infantry];
    for _ in 0..(num_squads - 1) {
        roles.push(extra_roles[(rng.next() as usize) % extra_roles.len()]);
    }

    // Determine x-range based on spawn side
    let (x_min, x_max) = match spawn_side {
        SpawnSide::West => (2u32, SPAWN_MARGIN - 2),
        SpawnSide::East => (GRID_SIZE - SPAWN_MARGIN + 2, GRID_SIZE - 2),
    };

    // Vertical center of map
    let y_center = GRID_SIZE / 2;

    let mut squads = Vec::new();
    let mut all_specs = Vec::new();
    let mut occupied: Vec<(u32, u32)> = Vec::new();
    let mut next_squad_id: SquadId = 1;

    // Pick which squad gets the player (frontline: Infantry or Skirmisher)
    let player_squad_idx = if has_player {
        let frontline_indices: Vec<usize> = roles
            .iter()
            .enumerate()
            .filter(|(_, r)| **r == SquadRole::Infantry || **r == SquadRole::Skirmisher)
            .map(|(i, _)| i)
            .collect();
        if frontline_indices.is_empty() {
            Some(0) // fallback to first squad
        } else {
            Some(frontline_indices[(rng.next() as usize) % frontline_indices.len()])
        }
    } else {
        None
    };

    for (squad_idx, role) in roles.iter().enumerate() {
        let squad_id = next_squad_id;
        next_squad_id += 1;

        let composition = role.composition();

        // Determine squad placement band
        let squad_x = match (spawn_side, role) {
            // Front units closer to center
            (SpawnSide::West, SquadRole::Infantry | SquadRole::Skirmisher) => x_max - 1,
            (SpawnSide::West, _) => x_min + 1,
            (SpawnSide::East, SquadRole::Infantry | SquadRole::Skirmisher) => x_min + 1,
            (SpawnSide::East, _) => x_max - 1,
        };

        // Vertical offset: spread squads vertically
        let y_offset = match squad_idx {
            0 => 0i32,
            1 => -6,
            2 => 6,
            _ => -12,
        };
        let squad_y_center = (y_center as i32 + y_offset).clamp(3, GRID_SIZE as i32 - 4) as u32;

        let mut unit_ids = Vec::new();

        for (unit_idx, &kind) in composition.iter().enumerate() {
            let is_player = player_squad_idx == Some(squad_idx) && unit_idx == 0;

            // Place unit near squad center, spreading vertically
            let base_y = squad_y_center as i32 + unit_idx as i32 - (composition.len() as i32 / 2);
            let base_x = squad_x as i32;

            let (gx, gy) = find_passable_tile(grid, base_x, base_y, &occupied);

            occupied.push((gx, gy));
            // unit_id will be assigned by Game::spawn_unit; use placeholder 0
            unit_ids.push(0);

            all_specs.push(UnitSpec {
                kind,
                faction,
                grid_x: gx,
                grid_y: gy,
                is_player,
                squad_id,
            });
        }

        squads.push(Squad {
            id: squad_id,
            faction,
            unit_ids,
            order: Order::Advance,
            target: None,
            leader_id: None, // set after spawning
        });
    }

    let commander = Commander {
        personality: Personality::Aggressive,
        portrait_index: (rng.next() % 25) as u8,
    };

    let army = Army {
        faction,
        commander,
        squads,
    };

    (army, all_specs)
}

/// Find a passable, unoccupied tile near (base_x, base_y) using spiral search.
fn find_passable_tile(
    grid: &Grid,
    base_x: i32,
    base_y: i32,
    occupied: &[(u32, u32)],
) -> (u32, u32) {
    // Spiral outward from base position
    for radius in 0..10 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs() != radius && dy.abs() != radius {
                    continue; // only check perimeter of this radius
                }
                let x = base_x + dx;
                let y = base_y + dy;
                if x < 0 || y < 0 || x >= GRID_SIZE as i32 || y >= GRID_SIZE as i32 {
                    continue;
                }
                let (ux, uy) = (x as u32, y as u32);
                if grid.is_passable(ux, uy) && !occupied.iter().any(|&(ox, oy)| ox == ux && oy == uy)
                {
                    return (ux, uy);
                }
            }
        }
    }
    // Fallback: return base clamped to grid (shouldn't happen with 12-tile spawn margin)
    let fx = base_x.clamp(0, GRID_SIZE as i32 - 1) as u32;
    let fy = base_y.clamp(0, GRID_SIZE as i32 - 1) as u32;
    (fx, fy)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test army::tests 2>&1 | tail -10`
Expected: all 10 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/army.rs
git commit -m "feat: implement procedural army generation"
```

### Task 7: Replace setup_demo_battle with setup_battle

**Files:**
- Modify: `src/game.rs` (replace setup_demo_battle_with_seed)
- Modify: `src/lib.rs:52` (call site)

- [ ] **Step 1: Write test for setup_battle**

Add to `src/game.rs` tests:

```rust
#[test]
fn setup_battle_spawns_two_armies() {
    let mut game = Game::new(960.0, 640.0);
    game.setup_battle(42);
    assert_eq!(game.armies.len(), 2);
    assert!(game.player_unit().is_some(), "Player should exist");
    // Both factions should have units
    let blue_count = game.units.iter().filter(|u| u.faction == Faction::Blue).count();
    let red_count = game.units.iter().filter(|u| u.faction == Faction::Red).count();
    assert!(blue_count >= 10, "Blue army too small: {blue_count}");
    assert!(red_count >= 10, "Red army too small: {red_count}");
}

#[test]
fn setup_battle_deterministic() {
    let mut g1 = Game::new(960.0, 640.0);
    g1.setup_battle(42);
    let mut g2 = Game::new(960.0, 640.0);
    g2.setup_battle(42);
    assert_eq!(g1.units.len(), g2.units.len());
    for (a, b) in g1.units.iter().zip(g2.units.iter()) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.grid_x, b.grid_x);
        assert_eq!(a.grid_y, b.grid_y);
        assert_eq!(a.faction, b.faction);
    }
}

#[test]
fn setup_battle_units_have_squad_ids() {
    let mut game = Game::new(960.0, 640.0);
    game.setup_battle(42);
    for unit in &game.units {
        assert!(unit.squad_id.is_some(), "Unit {} has no squad_id", unit.id);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test game::tests::setup_battle 2>&1 | tail -5`
Expected: FAIL — `setup_battle` doesn't exist

- [ ] **Step 3: Implement setup_battle, remove setup_demo_battle**

Add import at top of `src/game.rs`:

```rust
use crate::army::{self, Army, SpawnSide};
```

Replace `setup_demo_battle` and `setup_demo_battle_with_seed` methods (lines 704-829) with:

```rust
pub fn setup_battle(&mut self, seed: u32) {
    self.grid = mapgen::generate_battlefield(seed);

    let mut rng = mapgen::LcgRng::new(seed as u64 ^ 0xA2B1_C3D4);

    // Generate Blue army (player side, west)
    let (mut blue_army, blue_specs) =
        army::generate_army(Faction::Blue, &mut rng, SpawnSide::West, &self.grid, true);

    // Generate Red army (enemy side, east)
    let (mut red_army, red_specs) =
        army::generate_army(Faction::Red, &mut rng, SpawnSide::East, &self.grid, false);

    // Clear placeholder unit_ids — we'll rebuild them with real IDs
    for squad in &mut blue_army.squads {
        squad.unit_ids.clear();
    }
    for squad in &mut red_army.squads {
        squad.unit_ids.clear();
    }

    // Spawn Blue units and record their actual IDs
    for spec in &blue_specs {
        let unit_id = self.spawn_unit(
            spec.kind,
            spec.faction,
            spec.grid_x,
            spec.grid_y,
            spec.is_player,
            Some(spec.squad_id),
        );
        if let Some(squad) = blue_army.squads.iter_mut().find(|s| s.id == spec.squad_id) {
            squad.unit_ids.push(unit_id);
            if squad.leader_id.is_none() {
                squad.leader_id = Some(unit_id);
            }
        }
    }

    // Spawn Red units
    for spec in &red_specs {
        let unit_id = self.spawn_unit(
            spec.kind,
            spec.faction,
            spec.grid_x,
            spec.grid_y,
            spec.is_player,
            Some(spec.squad_id),
        );
        if let Some(squad) = red_army.squads.iter_mut().find(|s| s.id == spec.squad_id) {
            squad.unit_ids.push(unit_id);
            if squad.leader_id.is_none() {
                squad.leader_id = Some(unit_id);
            }
        }
    }

    self.armies = vec![blue_army, red_army];

    // Camera starts centered on player
    if let Some(player) = self.player_unit() {
        let (cx, cy) = grid::grid_to_world(player.grid_x, player.grid_y);
        self.camera.x = cx;
        self.camera.y = cy;
    }
    self.camera.zoom = 1.5;

    self.compute_water_adjacency();
    self.compute_fov();
}
```

Note: The hex literal `0xA2B1_C3D4` won't compile — use a numeric constant instead: `0xA2B1_C3D4u64`.

- [ ] **Step 4: Update lib.rs to call setup_battle**

In `src/lib.rs`, change line 52 from:

```rust
game_state.setup_demo_battle();
```

to:

```rust
game_state.setup_battle(42);
```

- [ ] **Step 5: Run tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass. Some old tests that called `setup_demo_battle` will need updating if they still exist — but since the old method was only called in lib.rs, the game.rs tests use `Game::new()` + manual `spawn_unit` calls.

- [ ] **Step 6: Build WASM to verify**

Run: `cargo build --target wasm32-unknown-unknown 2>&1 | tail -3`
Expected: compiles successfully

- [ ] **Step 7: Commit**

```bash
git add src/game.rs src/lib.rs
git commit -m "feat: replace setup_demo_battle with procedural setup_battle"
```

---

## Chunk 3: Combat Abilities (Charge + Heal)

### Task 8: Implement can_charge and charge combat functions

**Files:**
- Modify: `src/combat.rs`

- [ ] **Step 1: Write tests for can_charge**

Add to `src/combat.rs` tests:

```rust
fn make_lancer(id: u32, faction: Faction, x: u32, y: u32) -> Unit {
    Unit::new(id, UnitKind::Lancer, faction, x, y, false)
}

fn make_pawn(id: u32, faction: Faction, x: u32, y: u32) -> Unit {
    Unit::new(id, UnitKind::Pawn, faction, x, y, false)
}

#[test]
fn can_charge_straight_line() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(2, Faction::Red, 8, 5),
    ];
    let result = can_charge(&units[0], &units[1], &grid, &units);
    assert!(result.is_some());
    let path = result.unwrap();
    assert_eq!(path.len(), 2); // (6,5), (7,5) — stops adjacent
}

#[test]
fn can_charge_diagonal() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(2, Faction::Red, 8, 8),
    ];
    let result = can_charge(&units[0], &units[1], &grid, &units);
    assert!(result.is_some());
    let path = result.unwrap();
    assert_eq!(path.len(), 2); // (6,6), (7,7)
}

#[test]
fn cannot_charge_not_straight_line() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(2, Faction::Red, 7, 6), // not on a straight line
    ];
    let result = can_charge(&units[0], &units[1], &grid, &units);
    assert!(result.is_none());
}

#[test]
fn cannot_charge_adjacent() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(2, Faction::Red, 6, 5),
    ];
    let result = can_charge(&units[0], &units[1], &grid, &units);
    assert!(result.is_none()); // too close, just melee
}

#[test]
fn cannot_charge_blocked_by_friendly() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(3, Faction::Blue, 6, 5), // blocking
        make_warrior(2, Faction::Red, 8, 5),
    ];
    let result = can_charge(&units[0], &units[2], &grid, &units);
    assert!(result.is_none());
}

#[test]
fn cannot_charge_blocked_by_terrain() {
    let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    grid.set(6, 5, TileKind::Water);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(2, Faction::Red, 8, 5),
    ];
    let result = can_charge(&units[0], &units[1], &grid, &units);
    assert!(result.is_none());
}

#[test]
fn cannot_charge_through_enemy() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let units = vec![
        make_lancer(1, Faction::Blue, 5, 5),
        make_warrior(3, Faction::Red, 7, 5),  // enemy blocking the path
        make_warrior(2, Faction::Red, 10, 5),
    ];
    // Charging toward far enemy, but closer enemy occupies a path tile
    let result = can_charge(&units[0], &units[2], &grid, &units);
    assert!(result.is_none()); // blocked by enemy unit on the path
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test combat::tests::can_charge 2>&1 | tail -5`
Expected: FAIL — `can_charge` doesn't exist

- [ ] **Step 3: Implement can_charge**

Add to `src/combat.rs`:

```rust
/// Check if a Lancer can charge in a straight line to an enemy.
/// Returns the path of tiles the Lancer traverses (NOT including start, NOT including target).
/// Returns None if: not a Lancer, not straight line, distance < 2, path blocked.
pub fn can_charge(
    attacker: &Unit,
    target: &Unit,
    grid: &Grid,
    all_units: &[Unit],
) -> Option<Vec<(u32, u32)>> {
    if attacker.kind != UnitKind::Lancer || !attacker.alive || !target.alive {
        return None;
    }
    if attacker.has_attacked || attacker.faction == target.faction {
        return None;
    }

    let dx = target.grid_x as i32 - attacker.grid_x as i32;
    let dy = target.grid_y as i32 - attacker.grid_y as i32;

    // Must be a straight line (cardinal or 45-degree diagonal)
    if dx == 0 && dy == 0 {
        return None;
    }
    if dx != 0 && dy != 0 && dx.abs() != dy.abs() {
        return None; // not on a straight line
    }

    let step_x = dx.signum();
    let step_y = dy.signum();
    let steps = dx.abs().max(dy.abs());

    if steps < 2 {
        return None; // too close, just melee
    }

    // Walk the path (exclude start tile, exclude target tile)
    let mut path = Vec::new();
    for i in 1..steps {
        let px = (attacker.grid_x as i32 + step_x * i) as u32;
        let py = (attacker.grid_y as i32 + step_y * i) as u32;

        if !grid.is_passable(px, py) {
            return None; // blocked by terrain
        }
        // Check for any unit (friendly or enemy) blocking the path
        if all_units.iter().any(|u| u.alive && u.id != attacker.id && u.grid_x == px && u.grid_y == py) {
            return None; // blocked by unit
        }
        path.push((px, py));
    }

    Some(path)
}
```

- [ ] **Step 4: Run can_charge tests**

Run: `cargo test combat::tests::can_charge 2>&1 | tail -10`
Expected: all 7 charge tests pass

- [ ] **Step 5: Write tests for charge damage and execute_charge**

Add to `src/combat.rs` tests:

```rust
#[test]
fn charge_damage_includes_distance() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let lancer = make_lancer(1, Faction::Blue, 5, 5);
    let defender = make_warrior(2, Faction::Red, 8, 5);
    // Lancer ATK=4, distance=2, Warrior DEF=3 → 4+2-3 = 3
    let damage = calc_charge_damage(&lancer, &defender, &grid, 2);
    assert_eq!(damage, 3);
}

#[test]
fn execute_charge_moves_and_damages() {
    let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
    let mut lancer = make_lancer(1, Faction::Blue, 5, 5);
    let mut defender = make_warrior(2, Faction::Red, 8, 5);
    let path = vec![(6, 5), (7, 5)];
    let result = execute_charge(&mut lancer, &mut defender, &grid, &path);
    assert_eq!(lancer.grid_x, 7); // moved to last path tile
    assert_eq!(lancer.grid_y, 5);
    assert!(lancer.has_attacked);
    assert!(lancer.has_moved);
    assert!(result.damage > 0);
    assert!(defender.hp < defender.stats.max_hp);
}
```

- [ ] **Step 6: Implement calc_charge_damage and execute_charge**

Add to `src/combat.rs`:

```rust
/// Calculate charge damage: ATK + distance - DEF - terrain modifiers.
pub fn calc_charge_damage(attacker: &Unit, defender: &Unit, grid: &Grid, distance: u32) -> i32 {
    let terrain_def = grid.get(defender.grid_x, defender.grid_y).defense_bonus();
    let elev_def = grid.elevation_defense_bonus(defender.grid_x, defender.grid_y);
    (attacker.stats.atk + distance as i32 - defender.stats.def - terrain_def - elev_def).max(1)
}

/// Execute a Lancer charge: move along path, apply damage, set flags.
pub fn execute_charge(
    attacker: &mut Unit,
    defender: &mut Unit,
    grid: &Grid,
    path: &[(u32, u32)],
) -> CombatResult {
    let distance = path.len() as u32;
    if let Some(&(dest_x, dest_y)) = path.last() {
        attacker.grid_x = dest_x;
        attacker.grid_y = dest_y;
    }
    let damage = calc_charge_damage(attacker, defender, grid, distance);
    defender.take_damage(damage);
    attacker.has_attacked = true;
    attacker.has_moved = true;

    if defender.grid_x > attacker.grid_x {
        attacker.facing = Facing::Right;
    } else if defender.grid_x < attacker.grid_x {
        attacker.facing = Facing::Left;
    }
    attacker.set_anim(UnitAnim::Attack);

    CombatResult {
        damage,
        target_killed: !defender.alive,
    }
}
```

- [ ] **Step 7: Run all combat tests**

Run: `cargo test combat::tests 2>&1 | tail -10`
Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
git add src/combat.rs
git commit -m "feat: add Lancer Charge (can_charge, calc_charge_damage, execute_charge)"
```

### Task 9: Implement Monk Heal

**Files:**
- Modify: `src/combat.rs`

- [ ] **Step 1: Write tests for execute_heal**

Add to `src/combat.rs` tests:

```rust
fn make_monk(id: u32, faction: Faction, x: u32, y: u32) -> Unit {
    Unit::new(id, UnitKind::Monk, faction, x, y, false)
}

#[test]
fn execute_heal_restores_hp() {
    let mut healer = make_monk(1, Faction::Blue, 5, 5);
    let mut target = make_warrior(2, Faction::Blue, 6, 5);
    target.hp = 5; // missing 5 HP
    let healed = execute_heal(&mut healer, &mut target);
    assert_eq!(healed, 3);
    assert_eq!(target.hp, 8);
    assert!(healer.has_attacked);
}

#[test]
fn execute_heal_caps_at_max_hp() {
    let mut healer = make_monk(1, Faction::Blue, 5, 5);
    let mut target = make_warrior(2, Faction::Blue, 6, 5);
    target.hp = 9; // only missing 1 HP
    let healed = execute_heal(&mut healer, &mut target);
    assert_eq!(healed, 1);
    assert_eq!(target.hp, 10);
}

#[test]
fn execute_heal_no_overheal() {
    let mut healer = make_monk(1, Faction::Blue, 5, 5);
    let mut target = make_warrior(2, Faction::Blue, 6, 5);
    // Full HP
    let healed = execute_heal(&mut healer, &mut target);
    assert_eq!(healed, 0);
    assert_eq!(target.hp, 10);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test combat::tests::execute_heal 2>&1 | tail -5`
Expected: FAIL — `execute_heal` doesn't exist

- [ ] **Step 3: Implement execute_heal**

Add to `src/combat.rs`:

```rust
/// Execute a Monk heal on an adjacent ally. Returns actual HP restored.
pub fn execute_heal(healer: &mut Unit, target: &mut Unit) -> i32 {
    let heal_amount = 3.min(target.stats.max_hp - target.hp);
    target.hp += heal_amount;
    healer.has_attacked = true;
    healer.set_anim(UnitAnim::Attack);
    heal_amount
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test combat::tests::execute_heal 2>&1 | tail -5`
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/combat.rs
git commit -m "feat: add Monk Heal (execute_heal)"
```

---

## Chunk 4: Animation Events for Charge and Heal

### Task 10: Add Charge and Heal TurnEvent variants

**Files:**
- Modify: `src/animation.rs:14-36` (TurnEvent enum)

- [ ] **Step 1: Write tests for new TurnEvent variants**

Add to `src/animation.rs` tests:

```rust
#[test]
fn turn_event_charge_creation() {
    let event = TurnEvent::Charge {
        unit_id: 1,
        path: vec![(6, 5), (7, 5)],
        target_id: 2,
        damage: 5,
        killed: false,
    };
    match event {
        TurnEvent::Charge { unit_id, path, target_id, damage, killed } => {
            assert_eq!(unit_id, 1);
            assert_eq!(path.len(), 2);
            assert_eq!(target_id, 2);
            assert_eq!(damage, 5);
            assert!(!killed);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn turn_event_heal_creation() {
    let event = TurnEvent::Heal {
        healer_id: 1,
        target_id: 2,
        amount: 3,
    };
    match event {
        TurnEvent::Heal { healer_id, target_id, amount } => {
            assert_eq!(healer_id, 1);
            assert_eq!(target_id, 2);
            assert_eq!(amount, 3);
        }
        _ => panic!("wrong variant"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test animation::tests::turn_event_charge 2>&1 | tail -5`
Expected: FAIL — variant doesn't exist

- [ ] **Step 3: Add Charge and Heal variants to TurnEvent**

In `src/animation.rs`, add after `RangedAttack` variant (line 35):

```rust
Charge {
    unit_id: UnitId,
    path: Vec<(u32, u32)>,
    target_id: UnitId,
    damage: i32,
    killed: bool,
},
Heal {
    healer_id: UnitId,
    target_id: UnitId,
    amount: i32,
},
```

- [ ] **Step 4: Add AnimPhase variants for Charge and Heal**

In `src/animation.rs`, add after `RangedAttack` AnimPhase variant (line 91):

```rust
Charge {
    unit_id: UnitId,
    target_id: UnitId,
    #[allow(dead_code)]
    damage: i32,
    killed: bool,
    duration: f32,
    particle_spawned: bool,
},
Heal {
    healer_id: UnitId,
    target_id: UnitId,
    #[allow(dead_code)]
    amount: i32,
    duration: f32,
    particle_spawned: bool,
},
```

- [ ] **Step 5: Handle new variants in enqueue()**

In `src/animation.rs`, inside the `enqueue` method's match block (around line 173, before the closing `}`):

```rust
TurnEvent::Charge {
    unit_id,
    path,
    target_id,
    damage,
    killed,
} => {
    // Spawn dust at each path tile
    for &(px, py) in &path {
        let (fx, fy) = grid::grid_to_world(px, py);
        output.particles.push((ParticleKind::Dust, fx, fy));
    }
    // Charge is faster than normal attack
    let charge_duration = 0.25;
    self.phases.push_back(AnimPhase::Charge {
        unit_id,
        target_id,
        damage,
        killed,
        duration: charge_duration,
        particle_spawned: false,
    });
}
TurnEvent::Heal {
    healer_id,
    target_id,
    amount,
} => {
    self.phases.push_back(AnimPhase::Heal {
        healer_id,
        target_id,
        amount,
        duration: MELEE_ATTACK_DURATION,
        particle_spawned: false,
    });
}
```

- [ ] **Step 6: Handle new AnimPhase variants in update()**

In `src/animation.rs`, inside the `update` method's match block (around line 300, before `None => false,`):

```rust
Some(AnimPhase::Charge {
    unit_id,
    target_id,
    killed,
    duration,
    particle_spawned,
    ..
}) => {
    let unit_id = *unit_id;
    let target_id = *target_id;
    let killed = *killed;
    let duration = *duration;

    if let Some(unit) = units.iter_mut().find(|u| u.id == unit_id) {
        unit.set_anim(UnitAnim::Attack);
    }

    if !*particle_spawned && self.phase_elapsed >= duration * 0.5 {
        *particle_spawned = true;
        if let Some(defender) = units.iter().find(|u| u.id == target_id) {
            output.particles.push((
                ParticleKind::ExplosionLarge,
                defender.visual_x,
                defender.visual_y,
            ));
        }
    }

    let done = self.phase_elapsed >= duration;
    if done {
        if let Some(unit) = units.iter_mut().find(|u| u.id == unit_id) {
            unit.set_anim(UnitAnim::Idle);
        }
        if killed {
            self.visual_alive.remove(&target_id);
        }
    }
    done
}
Some(AnimPhase::Heal {
    healer_id,
    target_id,
    duration,
    particle_spawned,
    ..
}) => {
    let healer_id = *healer_id;
    let target_id = *target_id;
    let duration = *duration;

    if let Some(unit) = units.iter_mut().find(|u| u.id == healer_id) {
        unit.set_anim(UnitAnim::Attack);
    }

    if !*particle_spawned && self.phase_elapsed >= duration * 0.3 {
        *particle_spawned = true;
        if let Some(target) = units.iter().find(|u| u.id == target_id) {
            // Reuse Dust particle as a heal visual for now
            output.particles.push((
                ParticleKind::Dust,
                target.visual_x,
                target.visual_y,
            ));
        }
    }

    let done = self.phase_elapsed >= duration;
    if done {
        if let Some(unit) = units.iter_mut().find(|u| u.id == healer_id) {
            unit.set_anim(UnitAnim::Idle);
        }
    }
    done
}
```

- [ ] **Step 7: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
git add src/animation.rs
git commit -m "feat: add Charge and Heal animation events and phases"
```

---

## Chunk 5: Squad AI (Rewrite ai_turn)

### Task 11: Implement squad-based AI movement helper

**Files:**
- Modify: `src/army.rs` (add AI helper functions)

- [ ] **Step 1: Write tests for squad_centroid and move_score**

Add to `src/army.rs` tests:

```rust
use crate::unit::Unit;

#[test]
fn squad_centroid_calculation() {
    let units = vec![
        Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, false),
        Unit::new(2, UnitKind::Warrior, Faction::Blue, 7, 5, false),
        Unit::new(3, UnitKind::Warrior, Faction::Blue, 6, 7, false),
    ];
    let ids = vec![1, 2, 3];
    let (cx, cy) = squad_centroid(&ids, &units);
    assert_eq!(cx, 6); // (5+7+6)/3 = 6
    assert_eq!(cy, 5); // (5+5+7)/3 ≈ 5.67 → 5
}

#[test]
fn move_score_prefers_target() {
    let target = (10, 5);
    let centroid = (6, 5);
    // Tile closer to target should score higher
    let score_closer = move_score(8, 5, target, centroid);
    let score_farther = move_score(4, 5, target, centroid);
    assert!(score_closer > score_farther);
}

#[test]
fn move_score_penalizes_straying() {
    let target = (10, 5);
    let centroid = (6, 5);
    // Two tiles equidistant from target, but one is near centroid
    let score_near_squad = move_score(8, 5, target, centroid);
    let score_far_from_squad = move_score(8, 15, target, centroid);
    assert!(score_near_squad > score_far_from_squad);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test army::tests::squad_centroid 2>&1 | tail -5`
Expected: FAIL — function doesn't exist

- [ ] **Step 3: Implement squad_centroid and move_score**

Add to `src/army.rs`:

```rust
/// Compute the centroid (average position) of alive units in a squad.
pub fn squad_centroid(unit_ids: &[UnitId], units: &[crate::unit::Unit]) -> (u32, u32) {
    let mut sum_x = 0u32;
    let mut sum_y = 0u32;
    let mut count = 0u32;
    for &uid in unit_ids {
        if let Some(u) = units.iter().find(|u| u.id == uid && u.alive) {
            sum_x += u.grid_x;
            sum_y += u.grid_y;
            count += 1;
        }
    }
    if count == 0 {
        return (0, 0);
    }
    (sum_x / count, sum_y / count)
}

/// Score a candidate tile for squad-aware movement.
/// Higher is better.
pub fn move_score(
    x: u32,
    y: u32,
    target: (u32, u32),
    centroid: (u32, u32),
) -> i32 {
    let dist_to_target = (x as i32 - target.0 as i32).abs().max((y as i32 - target.1 as i32).abs());
    let dist_to_centroid = (x as i32 - centroid.0 as i32).abs().max((y as i32 - centroid.1 as i32).abs());
    -3 * dist_to_target - dist_to_centroid
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test army::tests 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/army.rs
git commit -m "feat: add squad AI helpers (centroid, move scoring)"
```

### Task 12: Rewrite ai_turn with squad-based dispatch

**Files:**
- Modify: `src/game.rs:561-664` (ai_turn method)

- [ ] **Step 1: Write tests for squad-based AI behavior**

Add to `src/game.rs` tests:

```rust
#[test]
fn ai_squad_units_advance_together() {
    use crate::army::{Army, Commander, Order, Personality, Squad};
    let mut game = Game::new(960.0, 640.0);
    // Player far away so AI doesn't attack
    game.spawn_unit(UnitKind::Warrior, Faction::Blue, 2, 32, true, None);
    // Red squad of 3 warriors in the east
    let r1 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 50, 30, false, Some(1));
    let r2 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 50, 31, false, Some(1));
    let r3 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 50, 32, false, Some(1));
    game.armies.push(Army {
        faction: Faction::Red,
        commander: Commander { personality: Personality::Aggressive, portrait_index: 0 },
        squads: vec![Squad {
            id: 1,
            faction: Faction::Red,
            unit_ids: vec![r1, r2, r3],
            order: Order::Advance,
            target: None,
            leader_id: Some(r1),
        }],
    });
    game.player_step(SwipeDir::E);
    // All three should have moved westward (toward player)
    for &uid in &[r1, r2, r3] {
        let u = game.find_unit(uid).unwrap();
        assert!(u.grid_x < 50, "Unit {uid} should have advanced west, x={}", u.grid_x);
    }
}

#[test]
fn ai_monk_heals_instead_of_attacking() {
    use crate::army::{Army, Commander, Order, Personality, Squad};
    let mut game = Game::new(960.0, 640.0);
    game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true, None);
    // Red warrior and monk adjacent
    let warrior_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 10, 10, false, Some(1));
    let monk_id = game.spawn_unit(UnitKind::Monk, Faction::Red, 11, 10, false, Some(1));
    // Damage the warrior so monk wants to heal
    game.units.iter_mut().find(|u| u.id == warrior_id).unwrap().hp = 3;
    game.armies.push(Army {
        faction: Faction::Red,
        commander: Commander { personality: Personality::Aggressive, portrait_index: 0 },
        squads: vec![Squad {
            id: 1,
            faction: Faction::Red,
            unit_ids: vec![warrior_id, monk_id],
            order: Order::Advance,
            target: None,
            leader_id: Some(warrior_id),
        }],
    });
    game.player_step(SwipeDir::E);
    // Monk should have healed the warrior
    let warrior = game.find_unit(warrior_id).unwrap();
    assert!(warrior.hp > 3, "Monk should have healed warrior, hp={}", warrior.hp);
}
```

- [ ] **Step 2: Run tests to verify current behavior (they may or may not pass with old AI)**

Run: `cargo test game::tests::ai_squad 2>&1 | tail -5`
Run: `cargo test game::tests::ai_monk 2>&1 | tail -5`
Expected: may fail since ai_turn doesn't know about squads yet

- [ ] **Step 3: Rewrite ai_turn**

Replace `ai_turn` method in `src/game.rs` (lines 561-664). Add the necessary imports at the top of the file:

```rust
use crate::army::{self, squad_centroid, move_score};
```

New `ai_turn` implementation:

```rust
/// Squad-based AI: each squad executes its order.
/// Player's squad acts first (minus the player), then remaining squads.
fn ai_turn(&mut self, position_snapshot: &[(UnitId, u32, u32)]) {
    // Collect all squad info: (squad_id, faction, order, unit_ids)
    // Process player's squad first
    let player_squad_id = self
        .player_unit()
        .and_then(|p| p.squad_id);

    let mut squad_infos: Vec<(u32, Faction, army::Order, Vec<UnitId>)> = Vec::new();

    // Player's squad first
    if let Some(psid) = player_squad_id {
        for army in &self.armies {
            for squad in &army.squads {
                if squad.id == psid {
                    squad_infos.push((squad.id, squad.faction, squad.order, squad.unit_ids.clone()));
                }
            }
        }
    }

    // Then all other squads
    for army in &self.armies {
        for squad in &army.squads {
            if Some(squad.id) != player_squad_id {
                squad_infos.push((squad.id, squad.faction, squad.order, squad.unit_ids.clone()));
            }
        }
    }

    // Collect orphaned unit indices (no squad or dead squad)
    let squadded_ids: std::collections::HashSet<UnitId> = squad_infos
        .iter()
        .flat_map(|(_, _, _, ids)| ids.iter().copied())
        .collect();

    for (_, faction, order, unit_ids) in &squad_infos {
        let faction = *faction;
        let order = *order;

        // Get alive non-player units in this squad
        let mut alive_ids: Vec<UnitId> = unit_ids
            .iter()
            .copied()
            .filter(|&uid| {
                self.units.iter().any(|u| u.id == uid && u.alive && !u.is_player)
            })
            .collect();

        if alive_ids.is_empty() {
            continue;
        }

        // Sort: melee first, then ranged, then monks
        alive_ids.sort_by_key(|&uid| {
            let u = self.units.iter().find(|u| u.id == uid).unwrap();
            match u.kind {
                UnitKind::Warrior | UnitKind::Lancer | UnitKind::Pawn => 0,
                UnitKind::Archer => 1,
                UnitKind::Monk => 2,
            }
        });

        let centroid = squad_centroid(unit_ids, &self.units);

        // Compute squad target: centroid of nearest enemy squad
        let squad_target = match order {
            army::Order::Advance => {
                self.nearest_enemy_squad_centroid(faction, centroid)
                    .unwrap_or(centroid)
            }
            army::Order::Hold => centroid,
        };

        for uid in alive_ids {
            self.execute_unit_ai(uid, squad_target, centroid, position_snapshot);
        }
    }

    // Orphaned units: fallback to nearest-enemy behavior
    let orphan_indices: Vec<usize> = self
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.alive && !u.is_player && !squadded_ids.contains(&u.id))
        .map(|(i, _)| i)
        .collect();

    for idx in orphan_indices {
        let uid = self.units[idx].id;
        let fallback_target = self.nearest_enemy_pos(uid);
        if let Some(target) = fallback_target {
            self.execute_unit_ai(uid, target, target, position_snapshot);
        }
    }
}

/// Find centroid of the nearest enemy squad relative to a given position.
fn nearest_enemy_squad_centroid(&self, faction: Faction, from: (u32, u32)) -> Option<(u32, u32)> {
    let mut best: Option<(u32, (u32, u32))> = None;
    for army in &self.armies {
        if army.faction == faction {
            continue;
        }
        for squad in &army.squads {
            let alive: Vec<UnitId> = squad
                .unit_ids
                .iter()
                .copied()
                .filter(|&uid| self.units.iter().any(|u| u.id == uid && u.alive))
                .collect();
            if alive.is_empty() {
                continue;
            }
            let c = squad_centroid(&alive, &self.units);
            let dx = (from.0 as i32 - c.0 as i32).unsigned_abs();
            let dy = (from.1 as i32 - c.1 as i32).unsigned_abs();
            let dist = dx.max(dy); // Chebyshev distance from our position
            match &best {
                None => best = Some((dist, c)),
                Some((bd, _)) if dist < *bd => best = Some((dist, c)),
                _ => {}
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Find the position of the nearest enemy to a given unit.
fn nearest_enemy_pos(&self, unit_id: UnitId) -> Option<(u32, u32)> {
    let unit = self.units.iter().find(|u| u.id == unit_id)?;
    let faction = unit.faction;
    let ux = unit.grid_x;
    let uy = unit.grid_y;
    self.units
        .iter()
        .filter(|u| u.alive && u.faction != faction)
        .min_by_key(|u| {
            let dx = (ux as i32 - u.grid_x as i32).abs();
            let dy = (uy as i32 - u.grid_y as i32).abs();
            dx.max(dy)
        })
        .map(|u| (u.grid_x, u.grid_y))
}

/// Execute AI for a single unit: attack, charge, heal, or move based on unit type.
fn execute_unit_ai(
    &mut self,
    unit_id: UnitId,
    squad_target: (u32, u32),
    squad_centroid: (u32, u32),
    position_snapshot: &[(UnitId, u32, u32)],
) {
    let unit_idx = match self.units.iter().position(|u| u.id == unit_id) {
        Some(i) => i,
        None => return,
    };
    if !self.units[unit_idx].alive {
        return;
    }

    let kind = self.units[unit_idx].kind;
    let faction = self.units[unit_idx].faction;
    let ax = self.units[unit_idx].grid_x;
    let ay = self.units[unit_idx].grid_y;
    let range = self.units[unit_idx].stats.range;

    match kind {
        UnitKind::Monk => {
            // Heal adjacent ally below 60% HP, or move
            if let Some(heal_target_id) = self.find_heal_target(unit_id) {
                self.execute_heal_action(unit_id, heal_target_id);
            } else {
                self.move_unit_toward(unit_idx, squad_target, squad_centroid);
            }
        }
        UnitKind::Lancer => {
            // Try charge, then melee, then move
            if let Some((target_id, path)) = self.find_charge_target(unit_id) {
                self.execute_charge_action(unit_id, target_id, path);
            } else if let Some(enemy_id) = self.find_melee_target(unit_idx) {
                self.execute_attack(unit_id, enemy_id, None);
            } else {
                self.move_unit_toward(unit_idx, squad_target, squad_centroid);
            }
        }
        UnitKind::Archer => {
            // Ranged attack, then melee, then move
            if let Some(enemy_id) = self.find_ranged_target(unit_idx, position_snapshot) {
                let snap_pos = position_snapshot
                    .iter()
                    .find(|(id, _, _)| *id == enemy_id)
                    .map(|&(_, x, y)| (x, y));
                self.execute_attack(unit_id, enemy_id, snap_pos);
            } else if let Some(enemy_id) = self.find_melee_target(unit_idx) {
                self.execute_attack(unit_id, enemy_id, None);
            } else {
                self.move_unit_toward(unit_idx, squad_target, squad_centroid);
            }
        }
        UnitKind::Warrior | UnitKind::Pawn => {
            // Melee, then move
            if let Some(enemy_id) = self.find_melee_target(unit_idx) {
                self.execute_attack(unit_id, enemy_id, None);
            } else {
                self.move_unit_toward(unit_idx, squad_target, squad_centroid);
            }
        }
    }
}

/// Find adjacent ally below 60% HP for monk to heal.
fn find_heal_target(&self, healer_id: UnitId) -> Option<UnitId> {
    let healer = self.units.iter().find(|u| u.id == healer_id)?;
    let hx = healer.grid_x;
    let hy = healer.grid_y;
    let faction = healer.faction;

    self.units
        .iter()
        .filter(|u| {
            u.alive
                && u.id != healer_id
                && u.faction == faction
                && u.distance_to(hx, hy) <= 1
                && (u.hp as f32) < (u.stats.max_hp as f32 * 0.6)
        })
        .min_by_key(|u| u.hp)
        .map(|u| u.id)
}

/// Execute a heal action and record the TurnEvent.
fn execute_heal_action(&mut self, healer_id: UnitId, target_id: UnitId) {
    let healer_idx = self.units.iter().position(|u| u.id == healer_id);
    let target_idx = self.units.iter().position(|u| u.id == target_id);
    let (healer_idx, target_idx) = match (healer_idx, target_idx) {
        (Some(h), Some(t)) => (h, t),
        _ => return,
    };

    let (healer, target) = if healer_idx < target_idx {
        let (left, right) = self.units.split_at_mut(target_idx);
        (&mut left[healer_idx], &mut right[0])
    } else {
        let (left, right) = self.units.split_at_mut(healer_idx);
        (&mut right[0], &mut left[target_idx])
    };

    let amount = combat::execute_heal(healer, target);
    self.turn_events.push(TurnEvent::Heal {
        healer_id,
        target_id,
        amount,
    });
}

/// Find a charge target for a Lancer: scan nearby enemies for straight-line charge.
fn find_charge_target(&self, lancer_id: UnitId) -> Option<(UnitId, Vec<(u32, u32)>)> {
    let lancer = self.units.iter().find(|u| u.id == lancer_id)?;
    let faction = lancer.faction;

    // Check enemies within charge range (2-5 tiles)
    self.units
        .iter()
        .filter(|u| u.alive && u.faction != faction)
        .filter_map(|enemy| {
            combat::can_charge(lancer, enemy, &self.grid, &self.units)
                .map(|path| (enemy.id, path))
        })
        .min_by_key(|(_, path)| path.len()) // prefer closest charge target
}

/// Execute a charge action and record the TurnEvent.
fn execute_charge_action(&mut self, lancer_id: UnitId, target_id: UnitId, path: Vec<(u32, u32)>) {
    let lancer_idx = self.units.iter().position(|u| u.id == lancer_id);
    let target_idx = self.units.iter().position(|u| u.id == target_id);
    let (lancer_idx, target_idx) = match (lancer_idx, target_idx) {
        (Some(l), Some(t)) => (l, t),
        _ => return,
    };

    let from = (self.units[lancer_idx].grid_x, self.units[lancer_idx].grid_y);

    let (attacker, defender) = if lancer_idx < target_idx {
        let (left, right) = self.units.split_at_mut(target_idx);
        (&mut left[lancer_idx], &mut right[0])
    } else {
        let (left, right) = self.units.split_at_mut(lancer_idx);
        (&mut right[0], &mut left[target_idx])
    };

    let result = combat::execute_charge(attacker, defender, &self.grid, &path);

    // Record move event for the charge path (so animation lerps)
    if let Some(&last) = path.last() {
        self.turn_events.push(TurnEvent::Move {
            unit_id: lancer_id,
            from,
            to: last,
        });
    }

    self.turn_events.push(TurnEvent::Charge {
        unit_id: lancer_id,
        path,
        target_id,
        damage: result.damage,
        killed: result.target_killed,
    });
}

/// Find an adjacent enemy for melee attack.
fn find_melee_target(&self, unit_idx: usize) -> Option<UnitId> {
    let unit = &self.units[unit_idx];
    let faction = unit.faction;
    self.units
        .iter()
        .find(|u| u.alive && u.faction != faction && unit.distance_to(u.grid_x, u.grid_y) <= 1)
        .map(|u| u.id)
}

/// Find a ranged target within range.
fn find_ranged_target(
    &self,
    unit_idx: usize,
    _position_snapshot: &[(UnitId, u32, u32)],
) -> Option<UnitId> {
    let unit = &self.units[unit_idx];
    let faction = unit.faction;
    let range = unit.stats.range;
    self.units
        .iter()
        .filter(|u| u.alive && u.faction != faction && unit.distance_to(u.grid_x, u.grid_y) <= range)
        .min_by_key(|u| unit.distance_to(u.grid_x, u.grid_y))
        .map(|u| u.id)
}

/// Move a unit one tile toward squad target, respecting squad cohesion.
fn move_unit_toward(
    &mut self,
    unit_idx: usize,
    target: (u32, u32),
    centroid: (u32, u32),
) {
    let ax = self.units[unit_idx].grid_x;
    let ay = self.units[unit_idx].grid_y;
    let unit_id = self.units[unit_idx].id;

    let mut best = (ax, ay);
    let mut best_score = i32::MIN;

    for &(sdx, sdy) in &[
        (0i32, -1i32), (1, 0), (0, 1), (-1, 0),
        (1, -1), (1, 1), (-1, 1), (-1, -1),
    ] {
        let nx = ax as i32 + sdx;
        let ny = ay as i32 + sdy;
        if !self.grid.in_bounds(nx, ny) {
            continue;
        }
        let nx = nx as u32;
        let ny = ny as u32;
        if !self.grid.is_passable(nx, ny) {
            continue;
        }
        if !self.grid.can_move_diagonal(ax, ay, sdx, sdy) {
            continue;
        }
        if self.unit_at(nx, ny).is_some() {
            continue;
        }
        let score = move_score(nx, ny, target, centroid);
        if score > best_score {
            best_score = score;
            best = (nx, ny);
        }
    }

    if best != (ax, ay) {
        let unit = &mut self.units[unit_idx];
        unit.grid_x = best.0;
        unit.grid_y = best.1;
        if best.0 > ax {
            unit.facing = Facing::Right;
        } else if best.0 < ax {
            unit.facing = Facing::Left;
        }
        self.turn_events.push(TurnEvent::Move {
            unit_id,
            from: (ax, ay),
            to: (best.0, best.1),
        });
    }
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass. Some existing tests that don't set up armies will use orphan fallback behavior.

- [ ] **Step 5: Build WASM to verify**

Run: `cargo build --target wasm32-unknown-unknown 2>&1 | tail -3`
Expected: compiles successfully

- [ ] **Step 6: Commit**

```bash
git add src/game.rs src/army.rs
git commit -m "feat: rewrite ai_turn with squad-based dispatch and unit abilities"
```

---

## Chunk 6: Integration and Final Verification

### Task 13: Fix any remaining compilation issues and run full test suite

**Files:**
- Potentially any modified file

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: all tests pass. Fix any compilation errors.

- [ ] **Step 2: Build WASM**

Run: `cargo build --target wasm32-unknown-unknown 2>&1 | tail -5`
Expected: compiles with no errors (warnings OK)

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --target wasm32-unknown-unknown 2>&1 | tail -20`
Expected: no errors. Fix any warnings that are easy to address.

- [ ] **Step 4: Fix issues if any, commit**

```bash
git add -A
git commit -m "fix: resolve compilation and clippy issues"
```

### Task 14: Verify game runs in browser

- [ ] **Step 1: Build WASM release**

Run: `wasm-pack build --target web 2>&1 | tail -5`
Expected: builds successfully

- [ ] **Step 2: Serve and verify in browser**

Open the game in a browser and verify:
- Two armies spawn on opposite sides
- Units move in squad formations toward each other
- Lancers charge when they have straight-line targets
- Monks heal wounded allies
- Animation system handles new events correctly
- Player can still move and attack normally

- [ ] **Step 3: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: final integration fixes for army/squad AI"
```

### Review Checkpoint

At this point the full vertical slice is complete:
- Two procedurally generated armies with squad structure
- Squad-level AI with Advance orders
- Lancer Charge ability
- Monk Heal ability
- Animation events for charge and heal
- All existing functionality preserved
