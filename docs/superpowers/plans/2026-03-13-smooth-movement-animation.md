# Smooth Movement Animation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an animation queue system that records turn events during game logic, then plays them back as smooth interpolated movement with easing.

**Architecture:** Game logic resolves instantly (grid positions update). A `TurnAnimator` converts `TurnEvent`s into sequenced `AnimPhase`s played back with lerped `visual_x/y` on each Unit. Input is blocked during playback. Particles/projectiles are spawned by the animator instead of game logic.

**Tech Stack:** Rust, wasm-bindgen, Canvas 2D

**Spec:** `docs/superpowers/specs/2026-03-13-smooth-movement-animation-design.md`

---

## Chunk 1: Core Animation Module + Unit Visual Position

### Task 1: Add `visual_x/y` fields to Unit

**Files:**
- Modify: `src/unit.rs:139-190` (Unit struct + new())

- [ ] **Step 1: Write failing test for visual position initialization**

```rust
// In src/unit.rs tests
#[test]
fn unit_visual_position_initialized() {
    use crate::grid;
    let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 10, false);
    let (expected_x, expected_y) = grid::grid_to_world(5, 10);
    assert!((unit.visual_x - expected_x).abs() < f32::EPSILON);
    assert!((unit.visual_y - expected_y).abs() < f32::EPSILON);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test unit::tests::unit_visual_position_initialized -- --nocapture 2>&1 | tail -5`
Expected: FAIL — `visual_x` field doesn't exist

- [ ] **Step 3: Add visual_x/y fields to Unit struct and initialize in new()**

In `src/unit.rs`, add to `Unit` struct:

```rust
/// World-space visual X position (for animation interpolation).
pub visual_x: f32,
/// World-space visual Y position (for animation interpolation).
pub visual_y: f32,
```

In `Unit::new()`, add the import and compute initial values:

```rust
use crate::grid;
// ... in the Self { ... } block:
let (vx, vy) = grid::grid_to_world(grid_x, grid_y);
// then in struct literal:
visual_x: vx,
visual_y: vy,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test unit::tests::unit_visual_position_initialized -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Run all tests to check no regressions**

Run: `cargo test 2>&1 | tail -5`
Expected: all 101+ tests pass

- [ ] **Step 6: Commit**

```bash
git add src/unit.rs
git commit -m "feat(animation): add visual_x/y fields to Unit"
```

---

### Task 2: Create TurnEvent enum and animation module skeleton

**Files:**
- Create: `src/animation.rs`
- Modify: `src/lib.rs:1` (add `pub mod animation;`)

- [ ] **Step 1: Write failing test for TurnEvent creation**

Create `src/animation.rs` with tests only first:

```rust
use crate::unit::UnitId;

/// Events recorded during game logic execution for animation playback.
#[derive(Clone, Debug)]
pub enum TurnEvent {
    Move {
        unit_id: UnitId,
        from: (u32, u32),
        to: (u32, u32),
    },
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
    },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
        target_pos: (u32, u32),
        missed: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_event_move_creation() {
        let event = TurnEvent::Move { unit_id: 1, from: (5, 5), to: (6, 5) };
        match event {
            TurnEvent::Move { unit_id, from, to } => {
                assert_eq!(unit_id, 1);
                assert_eq!(from, (5, 5));
                assert_eq!(to, (6, 5));
            }
            _ => panic!("wrong variant"),
        }
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod animation;` to `src/lib.rs` after the existing module declarations.

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test animation::tests::turn_event_move_creation -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/animation.rs src/lib.rs
git commit -m "feat(animation): add TurnEvent enum and animation module"
```

---

### Task 3: Implement TurnAnimator with easing and phase playback

**Files:**
- Modify: `src/animation.rs`

- [ ] **Step 1: Write failing tests for easing and animator**

Add to `src/animation.rs` tests:

```rust
#[test]
fn ease_out_quad_boundaries() {
    assert!((ease_out_quad(0.0)).abs() < f32::EPSILON);
    assert!((ease_out_quad(1.0) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn ease_out_quad_midpoint() {
    let mid = ease_out_quad(0.5);
    assert!(mid > 0.5, "ease_out should be > linear at midpoint, got {mid}");
    assert!((mid - 0.75).abs() < f32::EPSILON);
}

#[test]
fn animator_not_playing_when_empty() {
    let animator = TurnAnimator::new();
    assert!(!animator.is_playing());
}

#[test]
fn animator_plays_move_events() {
    use crate::grid;
    use crate::unit::{Unit, UnitKind, Faction};

    let mut animator = TurnAnimator::new();
    let mut units = vec![
        Unit::new(1, UnitKind::Warrior, Faction::Blue, 6, 5, true),
    ];
    // Manually set visual to "from" position
    let (fx, fy) = grid::grid_to_world(5, 5);
    units[0].visual_x = fx;
    units[0].visual_y = fy;

    let events = vec![TurnEvent::Move { unit_id: 1, from: (5, 5), to: (6, 5) }];
    animator.enqueue(events, false);
    assert!(animator.is_playing());

    // Advance past the full duration (MOVE_DURATION_SINGLE = 0.25s)
    let _output = animator.update(0.3, &mut units);
    assert!(!animator.is_playing());

    // Unit visual should be at destination
    let (ex, ey) = grid::grid_to_world(6, 5);
    assert!((units[0].visual_x - ex).abs() < 1.0);
    assert!((units[0].visual_y - ey).abs() < 1.0);
}

#[test]
fn animator_staggered_ai_moves() {
    use crate::grid;
    use crate::unit::{Unit, UnitKind, Faction};

    let mut animator = TurnAnimator::new();
    let mut units = vec![
        Unit::new(1, UnitKind::Warrior, Faction::Blue, 6, 5, true),
        Unit::new(2, UnitKind::Warrior, Faction::Red, 9, 5, false),
        Unit::new(3, UnitKind::Warrior, Faction::Red, 8, 8, false),
    ];
    let (f1x, f1y) = grid::grid_to_world(5, 5);
    units[0].visual_x = f1x;
    units[0].visual_y = f1y;
    let (f2x, f2y) = grid::grid_to_world(10, 5);
    units[1].visual_x = f2x;
    units[1].visual_y = f2y;
    let (f3x, f3y) = grid::grid_to_world(9, 8);
    units[2].visual_x = f3x;
    units[2].visual_y = f3y;

    let events = vec![
        TurnEvent::Move { unit_id: 1, from: (5, 5), to: (6, 5) },
        TurnEvent::Move { unit_id: 2, from: (10, 5), to: (9, 5) },
        TurnEvent::Move { unit_id: 3, from: (9, 8), to: (8, 8) },
    ];
    animator.enqueue(events, false);
    assert!(animator.is_playing());

    // Advance enough for all staggered moves to complete
    // total_duration = max(0.0 + 0.25, 0.05 + 0.25, 0.10 + 0.25) = 0.35
    let _output = animator.update(0.4, &mut units);
    assert!(!animator.is_playing());
}

#[test]
fn animator_melee_attack_phase() {
    use crate::unit::{Unit, UnitKind, Faction};

    let mut animator = TurnAnimator::new();
    let mut units = vec![
        Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, true),
        Unit::new(2, UnitKind::Warrior, Faction::Red, 6, 5, false),
    ];

    let events = vec![TurnEvent::MeleeAttack {
        attacker_id: 1,
        defender_id: 2,
        damage: 3,
        killed: false,
    }];
    animator.enqueue(events, false);
    assert!(animator.is_playing());

    // Advance past MELEE_ATTACK_DURATION (0.35s)
    let output = animator.update(0.4, &mut units);
    assert!(!animator.is_playing());
    // Should have spawned an ExplosionSmall particle
    assert!(!output.particles.is_empty());
}

#[test]
fn animator_visual_alive_tracks_kills() {
    use crate::unit::{Unit, UnitKind, Faction};

    let mut animator = TurnAnimator::new();
    let mut units = vec![
        Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, true),
        Unit::new(2, UnitKind::Warrior, Faction::Red, 6, 5, false),
    ];
    // Mark unit 2 as dead in game state (instant resolution)
    units[1].alive = false;
    units[1].death_fade = 0.3;

    // Must init visual_alive before enqueue so unit 2 starts as visually alive
    animator.init_visual_alive([1, 2].into_iter());

    let events = vec![TurnEvent::MeleeAttack {
        attacker_id: 1,
        defender_id: 2,
        damage: 10,
        killed: true,
    }];
    animator.enqueue(events, false);

    // Before phase completes, unit 2 should be visually alive
    assert!(animator.is_visually_alive(2));

    // After phase completes, unit 2 should no longer be visually alive
    let _output = animator.update(0.4, &mut units);
    assert!(!animator.is_visually_alive(2));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test animation::tests -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `TurnAnimator` not defined

- [ ] **Step 3: Implement TurnAnimator, AnimPhase, MoveAnim, easing**

Add to `src/animation.rs` (above tests):

```rust
use std::collections::{HashSet, VecDeque};
use crate::grid;
use crate::particle::ParticleKind;
use crate::unit::{Unit, UnitAnim, UnitId};

// Timing constants
const MOVE_DURATION_SINGLE: f32 = 0.25;
const MOVE_DURATION_AUTO: f32 = 0.15;
const AI_STAGGER_DELAY: f32 = 0.05;
const MELEE_ATTACK_DURATION: f32 = 0.35;
const RANGED_ATTACK_DURATION: f32 = 0.5;

/// Easing: deceleration curve.
pub fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

pub struct MoveAnim {
    pub unit_id: UnitId,
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub delay: f32,
    pub duration: f32,
}

pub enum AnimPhase {
    ParallelMoves {
        moves: Vec<MoveAnim>,
        total_duration: f32,
    },
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
        duration: f32,
        particle_spawned: bool,
    },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        target_pos: (f32, f32),
        missed: bool,
        damage: i32,
        killed: bool,
        duration: f32,
        projectile_spawned: bool,
    },
}

pub struct AnimOutput {
    pub particles: Vec<(ParticleKind, f32, f32)>,
    pub projectiles: Vec<(f32, f32, f32, f32)>,
}

impl AnimOutput {
    fn new() -> Self {
        Self {
            particles: Vec::new(),
            projectiles: Vec::new(),
        }
    }
}

pub struct TurnAnimator {
    phases: VecDeque<AnimPhase>,
    phase_elapsed: f32,
    visual_alive: HashSet<UnitId>,
    /// Track which moves have started (for dust spawning).
    move_started: Vec<bool>,
}

impl TurnAnimator {
    pub fn new() -> Self {
        Self {
            phases: VecDeque::new(),
            phase_elapsed: 0.0,
            visual_alive: HashSet::new(),
            move_started: Vec::new(),
        }
    }

    pub fn enqueue(&mut self, events: Vec<TurnEvent>, is_auto_move: bool) {
        // Initialize visual_alive with all unit IDs from events
        // (caller should also call init_visual_alive before enqueue for full set)
        let move_duration = if is_auto_move {
            MOVE_DURATION_AUTO
        } else {
            MOVE_DURATION_SINGLE
        };

        // Collect moves
        let mut moves: Vec<MoveAnim> = Vec::new();
        let mut melee_attacks: Vec<TurnEvent> = Vec::new();
        let mut ranged_attacks: Vec<TurnEvent> = Vec::new();

        for event in events {
            match event {
                TurnEvent::Move { unit_id, from, to } => {
                    let delay = if moves.is_empty() {
                        0.0 // first move (player) gets no delay
                    } else {
                        moves.len() as f32 * AI_STAGGER_DELAY
                    };
                    let (fx, fy) = grid::grid_to_world(from.0, from.1);
                    let (tx, ty) = grid::grid_to_world(to.0, to.1);
                    moves.push(MoveAnim {
                        unit_id,
                        from: (fx, fy),
                        to: (tx, ty),
                        delay,
                        duration: move_duration,
                    });
                }
                TurnEvent::MeleeAttack { .. } => melee_attacks.push(event),
                TurnEvent::RangedAttack { .. } => ranged_attacks.push(event),
            }
        }

        // Phase 1: parallel moves
        if !moves.is_empty() {
            let total_duration = moves
                .iter()
                .map(|m| m.delay + m.duration)
                .fold(0.0_f32, f32::max);
            self.phases.push_back(AnimPhase::ParallelMoves {
                moves,
                total_duration,
            });
        }

        // Phase 2: melee attacks (sequential)
        for event in melee_attacks {
            if let TurnEvent::MeleeAttack {
                attacker_id,
                defender_id,
                damage,
                killed,
            } = event
            {
                self.phases.push_back(AnimPhase::MeleeAttack {
                    attacker_id,
                    defender_id,
                    damage,
                    killed,
                    duration: MELEE_ATTACK_DURATION,
                    particle_spawned: false,
                });
            }
        }

        // Phase 3: ranged attacks (sequential)
        for event in ranged_attacks {
            if let TurnEvent::RangedAttack {
                attacker_id,
                defender_id,
                damage,
                killed,
                target_pos,
                missed,
            } = event
            {
                let (tx, ty) = grid::grid_to_world(target_pos.0, target_pos.1);
                self.phases.push_back(AnimPhase::RangedAttack {
                    attacker_id,
                    defender_id,
                    target_pos: (tx, ty),
                    missed,
                    damage,
                    killed,
                    duration: RANGED_ATTACK_DURATION,
                    projectile_spawned: false,
                });
            }
        }

        if !self.phases.is_empty() {
            self.phase_elapsed = 0.0;
            self.move_started.clear();
            // Initialize move_started for first phase if it's ParallelMoves
            if let Some(AnimPhase::ParallelMoves { moves, .. }) = self.phases.front() {
                self.move_started = vec![false; moves.len()];
            }
        }
    }

    /// Initialize the set of visually alive units before enqueueing events.
    pub fn init_visual_alive(&mut self, alive_ids: impl Iterator<Item = UnitId>) {
        self.visual_alive.clear();
        self.visual_alive.extend(alive_ids);
    }

    pub fn update(&mut self, dt: f32, units: &mut [Unit]) -> AnimOutput {
        let mut output = AnimOutput::new();

        if self.phases.is_empty() {
            return output;
        }

        self.phase_elapsed += dt;

        let phase_done = match self.phases.front_mut() {
            Some(AnimPhase::ParallelMoves {
                moves,
                total_duration,
            }) => {
                for (i, mv) in moves.iter().enumerate() {
                    let raw_t = if mv.duration > 0.0 {
                        ((self.phase_elapsed - mv.delay) / mv.duration).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let t = ease_out_quad(raw_t);

                    // Spawn dust when move starts
                    if i < self.move_started.len()
                        && !self.move_started[i]
                        && self.phase_elapsed >= mv.delay
                    {
                        self.move_started[i] = true;
                        output.particles.push((ParticleKind::Dust, mv.from.0, mv.from.1));
                    }

                    // Lerp visual position
                    if let Some(unit) = units.iter_mut().find(|u| u.id == mv.unit_id) {
                        unit.visual_x = mv.from.0 + (mv.to.0 - mv.from.0) * t;
                        unit.visual_y = mv.from.1 + (mv.to.1 - mv.from.1) * t;

                        if raw_t > 0.0 && raw_t < 1.0 {
                            unit.set_anim(UnitAnim::Run);
                        } else if raw_t >= 1.0 {
                            unit.set_anim(UnitAnim::Idle);
                        }
                    }
                }
                self.phase_elapsed >= *total_duration
            }
            Some(AnimPhase::MeleeAttack {
                attacker_id,
                defender_id,
                killed,
                duration,
                particle_spawned,
                ..
            }) => {
                let attacker_id = *attacker_id;
                let defender_id = *defender_id;
                let killed = *killed;
                let duration = *duration;

                // Set attacker anim to Attack
                if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                    unit.set_anim(UnitAnim::Attack);
                }

                // At 60% through, spawn damage particle
                if !*particle_spawned && self.phase_elapsed >= duration * 0.6 {
                    *particle_spawned = true;
                    if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                        let (wx, wy) = (defender.visual_x, defender.visual_y);
                        output
                            .particles
                            .push((ParticleKind::ExplosionSmall, wx, wy));
                    }
                }

                let done = self.phase_elapsed >= duration;
                if done {
                    // Reset attacker anim
                    if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                        unit.set_anim(UnitAnim::Idle);
                    }
                    if killed {
                        self.visual_alive.remove(&defender_id);
                        if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                            output.particles.push((
                                ParticleKind::ExplosionLarge,
                                defender.visual_x,
                                defender.visual_y,
                            ));
                        }
                    }
                }
                done
            }
            Some(AnimPhase::RangedAttack {
                attacker_id,
                defender_id,
                target_pos,
                missed,
                killed,
                duration,
                projectile_spawned,
                ..
            }) => {
                let attacker_id = *attacker_id;
                let defender_id = *defender_id;
                let target_pos = *target_pos;
                let missed = *missed;
                let killed = *killed;
                let duration = *duration;

                // Set attacker anim
                if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                    unit.set_anim(UnitAnim::Attack);
                }

                // Spawn projectile at phase start
                if !*projectile_spawned {
                    *projectile_spawned = true;
                    if let Some(attacker) = units.iter().find(|u| u.id == attacker_id) {
                        output.projectiles.push((
                            attacker.visual_x,
                            attacker.visual_y,
                            target_pos.0,
                            target_pos.1,
                        ));
                    }
                }

                let done = self.phase_elapsed >= duration;
                if done {
                    if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                        unit.set_anim(UnitAnim::Idle);
                    }
                    if missed {
                        output
                            .particles
                            .push((ParticleKind::Dust, target_pos.0, target_pos.1));
                    } else if killed {
                        self.visual_alive.remove(&defender_id);
                        if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                            output.particles.push((
                                ParticleKind::ExplosionLarge,
                                defender.visual_x,
                                defender.visual_y,
                            ));
                        }
                    }
                }
                done
            }
            None => false,
        };

        if phase_done {
            self.phases.pop_front();
            self.phase_elapsed = 0.0;
            // Reset move_started for next phase if it's ParallelMoves
            if let Some(AnimPhase::ParallelMoves { moves, .. }) = self.phases.front() {
                self.move_started = vec![false; moves.len()];
            } else {
                self.move_started.clear();
            }
        }

        output
    }

    pub fn is_playing(&self) -> bool {
        !self.phases.is_empty()
    }

    pub fn is_visually_alive(&self, unit_id: UnitId) -> bool {
        self.visual_alive.contains(&unit_id)
    }
}
```

- [ ] **Step 4: Run animation tests to verify they pass**

Run: `cargo test animation::tests -- --nocapture 2>&1 | tail -15`
Expected: all animation tests PASS

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/animation.rs
git commit -m "feat(animation): implement TurnAnimator with phase playback and easing"
```

---

## Chunk 2: Record Turn Events in Game Logic

### Task 4: Add turn_events to Game and record Move events

**Files:**
- Modify: `src/game.rs:1-8` (imports)
- Modify: `src/game.rs:12-34` (Game struct)
- Modify: `src/game.rs:36-61` (Game::new)
- Modify: `src/game.rs:99-170` (player_step — record moves)
- Modify: `src/game.rs:549-650` (ai_turn — record moves)

- [ ] **Step 1: Write failing test for turn events**

Add to `src/game.rs` tests:

```rust
#[test]
fn player_step_records_move_event() {
    use crate::animation::TurnEvent;
    let mut game = Game::new(960.0, 640.0);
    game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
    game.player_step(SwipeDir::E);
    // Should have at least one Move event for the player
    let has_player_move = game.turn_events.iter().any(|e| matches!(e, TurnEvent::Move { unit_id: 1, from: (5, 5), to: (6, 5) }));
    assert!(has_player_move, "Expected Move event for player, got: {:?}", game.turn_events);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test game::tests::player_step_records_move_event -- --nocapture 2>&1 | tail -5`
Expected: FAIL — `turn_events` field doesn't exist

- [ ] **Step 3: Add turn_events field and record events**

In `src/game.rs`:

1. Add import: `use crate::animation::TurnEvent;`

2. Add to `Game` struct: `pub turn_events: Vec<TurnEvent>,`

3. Add to `Game::new()`: `turn_events: Vec::new(),`

4. In `player_step()`, after the player moves (the `else` branch at line ~141-153), add before the dust particle:
```rust
self.turn_events.push(TurnEvent::Move {
    unit_id: player_id,
    from: (px, py),
    to: (nx, ny),
});
```

5. In `ai_turn()`, after an AI unit moves (the `if best != (ax, ay)` block at line ~635-647), add before the dust particle:
```rust
self.turn_events.push(TurnEvent::Move {
    unit_id: ai_id,
    from: (ax, ay),
    to: (best.0, best.1),
});
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test game::tests::player_step_records_move_event -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/game.rs
git commit -m "feat(animation): record Move turn events in player_step and ai_turn"
```

---

### Task 5: Record attack events in execute_attack

**Files:**
- Modify: `src/game.rs:480-547` (execute_attack)

- [ ] **Step 1: Write failing test for attack events**

Add to `src/game.rs` tests:

```rust
#[test]
fn player_step_records_melee_attack_event() {
    use crate::animation::TurnEvent;
    let mut game = Game::new(960.0, 640.0);
    game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
    game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
    game.player_step(SwipeDir::E);
    let has_melee = game.turn_events.iter().any(|e| matches!(e, TurnEvent::MeleeAttack { attacker_id: 1, .. }));
    assert!(has_melee, "Expected MeleeAttack event, got: {:?}", game.turn_events);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test game::tests::player_step_records_melee_attack_event -- --nocapture 2>&1 | tail -5`
Expected: FAIL — no MeleeAttack event found

- [ ] **Step 3: Record attack events in execute_attack**

In `execute_attack()`:

For the melee branch (after `execute_melee` call), add:
```rust
self.turn_events.push(TurnEvent::MeleeAttack {
    attacker_id,
    defender_id,
    damage: _result.damage,
    killed: _result.target_killed,
});
```
(Rename `_result` to `result` since we now use it.)

For the ranged hit branch (after `execute_ranged` call), add:
```rust
self.turn_events.push(TurnEvent::RangedAttack {
    attacker_id,
    defender_id,
    damage: result.damage,
    killed: result.target_killed,
    target_pos: (snap_x, snap_y),
    missed: false,
});
```

For the ranged miss branch (where `target_moved` is true), add:
```rust
self.turn_events.push(TurnEvent::RangedAttack {
    attacker_id,
    defender_id,
    damage: 0,
    killed: false,
    target_pos: (snap_x, snap_y),
    missed: true,
});
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test game::tests::player_step_records_melee_attack_event -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/game.rs
git commit -m "feat(animation): record MeleeAttack and RangedAttack turn events"
```

---

## Chunk 3: Remove Direct Particle/Projectile Spawning from Game Logic

### Task 6: Remove particle/projectile spawns from player_step, ai_turn, execute_attack

**Files:**
- Modify: `src/game.rs:99-170` (player_step — remove dust)
- Modify: `src/game.rs:549-650` (ai_turn — remove dust)
- Modify: `src/game.rs:480-547` (execute_attack — remove particles and projectiles)

- [ ] **Step 1: Remove dust particle from player_step move branch**

In `player_step()`, remove these lines from the movement `else` block:
```rust
let (wx, wy) = grid::grid_to_world(px, py);
self.particles.push(Particle::new(ParticleKind::Dust, wx, wy));
```

- [ ] **Step 2: Remove dust particle from ai_turn move**

In `ai_turn()`, remove these lines from the `if best != (ax, ay)` block:
```rust
let (wx, wy) = grid::grid_to_world(ax, ay);
self.particles.push(Particle::new(ParticleKind::Dust, wx, wy));
```

- [ ] **Step 3: Remove particles and projectiles from execute_attack**

In `execute_attack()`, remove:

Ranged branch — remove projectile spawn:
```rust
self.projectiles.push(Projectile::new(sx, sy, tx, ty));
```

Ranged miss — remove dust particle:
```rust
self.particles.push(Particle::new(ParticleKind::Dust, tx, ty));
```

Ranged hit kill — remove explosion:
```rust
self.particles.push(Particle::new(ParticleKind::ExplosionLarge, tx, ty));
```

Melee — remove explosion small:
```rust
self.particles.push(Particle::new(ParticleKind::ExplosionSmall, wx, wy));
```

Melee kill — remove explosion large:
```rust
if !defender.alive {
    self.particles.push(Particle::new(ParticleKind::ExplosionLarge, wx, wy));
}
```

Also remove the `let (wx, wy) = ...` line and `let (sx, sy) = ...` / `let (tx, ty) = ...` lines that are now unused.

**Note:** Keep the `let (sx, sy)` and `let (tx, ty)` in the ranged branch if they are still used for the snapshot position in the TurnEvent. If not used by any remaining code, remove them.

- [ ] **Step 4: Clean up unused imports**

If `Particle`, `ParticleKind`, or `Projectile` are no longer used in `game.rs`, remove them from the `use` statements at the top.

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/game.rs
git commit -m "refactor(animation): remove direct particle/projectile spawns from game logic"
```

---

## Chunk 4: Integrate Animator into Game Loop

### Task 7: Convert auto_move_timer to time-based

**Files:**
- Modify: `src/game.rs:12-34` (Game struct — change type)
- Modify: `src/game.rs:36-61` (Game::new — init)
- Modify: `src/game.rs:172-189` (set_auto_path — reset)
- Modify: `src/game_loop.rs:263-270` (auto-move processing)

- [ ] **Step 1: Change auto_move_timer from u32 to f32**

In `Game` struct, change: `pub auto_move_timer: f32,`
In `Game::new()`, change: `auto_move_timer: 0.0,`
In `set_auto_path()`, change: `self.auto_move_timer = 0.0;`

- [ ] **Step 2: Update game_loop.rs auto-move processing to time-based**

Replace the auto-move block in game_loop.rs (lines ~263-270):

```rust
// Process auto-move path (time-based: 0.15s per step)
if game.is_auto_moving() {
    game.auto_move_timer += dt as f32;
    if game.auto_move_timer >= 0.15 {
        game.auto_move_timer = 0.0;
        game.auto_move_step();
    }
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/game.rs src/game_loop.rs
git commit -m "refactor(animation): convert auto_move_timer to time-based (f32)"
```

---

### Task 8: Wire TurnAnimator into game_loop.rs

**Files:**
- Modify: `src/game_loop.rs:1-12` (imports)
- Modify: `src/game_loop.rs:1223-1231` (LoopState struct)
- Modify: `src/game_loop.rs:123-131` (LoopState construction in `run()`)
- Modify: `src/game_loop.rs:154-279` (game loop closure — input processing + update)

**Codebase note:** The game loop closure starts at line 154: `*g.borrow_mut() = Some(Closure::wrap(Box::new(move |timestamp: f64| {`. Inside it, state is accessed as `let mut state_guard = state.borrow_mut();` and game as `let game = &mut state_guard.game;`. The `LoopState` struct is at line 1223. All references below use `state_guard`.

- [ ] **Step 1: Add animator to LoopState**

Add import at top of `game_loop.rs`:
```rust
use crate::animation::TurnAnimator;
use crate::particle::{Particle, Projectile};
```

Add to `LoopState` struct (line 1223):
```rust
animator: TurnAnimator,
```

Initialize in the `LoopState` construction in `run()` (around line 123):
```rust
animator: TurnAnimator::new(),
```

- [ ] **Step 2: Block movement/action input while animator is playing**

**Important:** Only block movement/action input (arrow keys, swipe, click, long swipe, auto-move) — NOT camera controls (WASD pan, mouse wheel zoom, pinch-to-zoom, two-finger pan). Camera controls (lines ~173-196) must remain active during animation.

Wrap only the movement input block (lines ~207-234, starting from `// Arrow keys -> movement` through the click handler) inside:
```rust
if !state_guard.animator.is_playing() {
    // Arrow keys -> movement ...
    // Touch short swipe -> movement ...
    // Touch long swipe -> pathfinding ...
    // Mouse click -> step ...
}
```

- [ ] **Step 3: After input, compile turn events and enqueue**

After the input processing block closes (after `drop(inp)` / end of `input.borrow_mut()` scope), but still inside the `state_guard` scope, add:

```rust
// Compile turn events into animation phases
if !game.turn_events.is_empty() {
    let events = game.turn_events.drain(..).collect::<Vec<_>>();
    let is_auto = game.is_auto_moving();
    state_guard.animator.init_visual_alive(
        game.units.iter().filter(|u| u.alive).map(|u| u.id)
    );
    state_guard.animator.enqueue(events, is_auto);
}
```

- [ ] **Step 4: Each frame, advance animator and push spawned particles/projectiles**

Before `game.update(dt)` (line ~273), add:

```rust
// Advance animation and collect spawned effects
if state_guard.animator.is_playing() {
    let anim_output = state_guard.animator.update(dt as f32, &mut game.units);
    for (kind, x, y) in anim_output.particles {
        game.particles.push(Particle::new(kind, x, y));
    }
    for (sx, sy, tx, ty) in anim_output.projectiles {
        game.projectiles.push(Projectile::new(sx, sy, tx, ty));
    }
}
```

- [ ] **Step 5: Also block auto-move while animator is playing**

Update the auto-move block (line ~263):
```rust
if game.is_auto_moving() && !state_guard.animator.is_playing() {
    // ... existing auto-move timer logic ...
}
```

- [ ] **Step 6: Snap visual positions when not animating**

After the animator update block, add:
```rust
// When not animating, snap visual positions to grid positions
if !state_guard.animator.is_playing() {
    for unit in &mut game.units {
        let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
        unit.visual_x = wx;
        unit.visual_y = wy;
    }
}
```

- [ ] **Step 7: Build check**

Run: `cargo check 2>&1 | tail -10`
Expected: no errors

- [ ] **Step 8: Commit**

```bash
git add src/game_loop.rs
git commit -m "feat(animation): wire TurnAnimator into game loop with input blocking"
```

---

## Chunk 5: Switch Rendering to Visual Positions + Camera Follow

### Task 9: Render units at visual_x/y instead of grid_to_world

**Files:**
- Modify: `src/game_loop.rs:960-1040` (draw_foreground — unit rendering)
- Modify: `src/game_loop.rs:920-956` (HP bar rendering)

- [ ] **Step 1: Update unit sprite rendering to use visual_x/y**

In `draw_foreground()`, replace:
```rust
let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
```
with:
```rust
let (wx, wy) = (unit.visual_x, unit.visual_y);
```

This occurs around line 1012.

- [ ] **Step 2: Update unit Y-sort to use visual_y**

Replace the sort comparator (around line 975):
```rust
unit_indices.sort_by(|&a, &b| {
    game.units[a]
        .grid_y
        .cmp(&game.units[b].grid_y)
        .then(game.units[a].grid_x.cmp(&game.units[b].grid_x))
});
```
with:
```rust
unit_indices.sort_by(|&a, &b| {
    game.units[a]
        .visual_y
        .partial_cmp(&game.units[b].visual_y)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(
            game.units[a]
                .visual_x
                .partial_cmp(&game.units[b].visual_x)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
});
```

- [ ] **Step 3: Update HP bar rendering to use visual_x/y**

In the HP bar loop (around line 927), replace:
```rust
let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
```
with:
```rust
let (wx, wy) = (unit.visual_x, unit.visual_y);
```

- [ ] **Step 4: Build check**

Run: `cargo check 2>&1 | tail -10`
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add src/game_loop.rs
git commit -m "feat(animation): render units at visual_x/y positions"
```

---

### Task 10: Use visual_alive for unit visibility during animation

**Files:**
- Modify: `src/game_loop.rs:626` (`render_frame` signature)
- Modify: `src/game_loop.rs:679` (`draw_foreground` call in `render_frame`)
- Modify: `src/game_loop.rs:961` (`draw_foreground` signature and unit filter)
- Modify: `src/game_loop.rs:922-956` (HP bar rendering — also needs visual_alive)

**Codebase note:** `draw_foreground` is called at line 679 from `render_frame(state, loaded, preview_path)`. `render_frame` takes `&mut LoopState`, so it has access to `state.animator`. We pass the animator from `render_frame` to `draw_foreground`.

- [ ] **Step 1: Update draw_foreground signature**

```rust
fn draw_foreground(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    animator: &TurnAnimator,
) -> Result<(), JsValue> {
```

- [ ] **Step 2: Filter units using visual_alive when animating**

Update the unit filter (around line 968-974):
```rust
let mut unit_indices: Vec<usize> = game
    .units
    .iter()
    .enumerate()
    .filter(|(_, u)| {
        if animator.is_playing() {
            return animator.is_visually_alive(u.id) || u.death_fade > 0.0;
        }
        u.alive || u.death_fade > 0.0
    })
    .map(|(i, _)| i)
    .collect();
```

- [ ] **Step 3: Update HP bar rendering to also use visual_alive**

In `draw_overlays` (or wherever HP bars are drawn, around line 922), update the filter:
```rust
for unit in &game.units {
    // During animation, use visual_alive; otherwise use unit.alive
    let show = if animator.is_playing() {
        animator.is_visually_alive(unit.id)
    } else {
        unit.alive
    };
    if !show {
        continue;
    }
    // ... existing HP bar code ...
```

This requires also passing `&TurnAnimator` to `draw_overlays`. Update its signature and call site in `render_frame` similarly.

- [ ] **Step 4: Update call sites in render_frame**

At line 679, change:
```rust
draw_foreground(ctx, game, loaded, tm)?;
```
to:
```rust
draw_foreground(ctx, game, loaded, tm, &state.animator)?;
```

Similarly update the `draw_overlays` call at line 676 to pass `&state.animator`.

- [ ] **Step 5: Build check**

Run: `cargo check 2>&1 | tail -10`
Expected: no errors

- [ ] **Step 6: Commit**

```bash
git add src/game_loop.rs
git commit -m "feat(animation): use visual_alive for unit and HP bar rendering during animation"
```

---

### Task 11: Camera follows visual position

**Files:**
- Modify: `src/game.rs:652-680` (Game::update — camera follow)

- [ ] **Step 1: Update camera follow to use visual_x/y**

In `Game::update()`, replace:
```rust
if let Some(player) = self.player_unit() {
    let (tx, ty) = grid::grid_to_world(player.grid_x, player.grid_y);
    let lerp = (dt as f32 * 5.0).min(1.0);
    self.camera.x += (tx - self.camera.x) * lerp;
    self.camera.y += (ty - self.camera.y) * lerp;
}
```
with:
```rust
if let Some(player) = self.player_unit() {
    let lerp = (dt as f32 * 5.0).min(1.0);
    self.camera.x += (player.visual_x - self.camera.x) * lerp;
    self.camera.y += (player.visual_y - self.camera.y) * lerp;
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 3: Build WASM**

Run: `wasm-pack build --target web 2>&1 | tail -5`
Expected: clean build

- [ ] **Step 4: Commit**

```bash
git add src/game.rs
git commit -m "feat(animation): camera follows player visual position"
```

---

## Chunk 6: Sync visual_x/y in spawn_unit and Final Polish

### Task 12: Sync visual positions in spawn_unit

**Files:**
- Modify: `src/game.rs:63-76` (spawn_unit)

The `Unit::new()` already computes `visual_x/y` from `grid_to_world(grid_x, grid_y)` (added in Task 1), so `spawn_unit` needs no changes. However, verify this is correct.

- [ ] **Step 1: Write test confirming spawned units have correct visual pos**

Add to `src/game.rs` tests:

```rust
#[test]
fn spawned_unit_has_correct_visual_position() {
    use crate::grid;
    let mut game = Game::new(960.0, 640.0);
    let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 15, true);
    let unit = game.find_unit(id).unwrap();
    let (expected_x, expected_y) = grid::grid_to_world(10, 15);
    assert!((unit.visual_x - expected_x).abs() < f32::EPSILON);
    assert!((unit.visual_y - expected_y).abs() < f32::EPSILON);
}
```

- [ ] **Step 2: Run test**

Run: `cargo test game::tests::spawned_unit_has_correct_visual_position -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/game.rs
git commit -m "test(animation): verify spawned units have correct visual positions"
```

---

### Task 13: Clear turn_events after drain in player_step context

**Files:**
- Modify: `src/game.rs:99-170` (player_step)

The `turn_events` accumulate across `player_step` and its internal `ai_turn` call. They are drained in `game_loop.rs` (Task 8). Verify that `turn_events` are properly cleared.

- [ ] **Step 1: Write test verifying turn_events accumulate correctly**

Add to `src/game.rs` tests:

```rust
#[test]
fn turn_events_accumulate_player_and_ai() {
    use crate::animation::TurnEvent;
    let mut game = Game::new(960.0, 640.0);
    game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
    game.spawn_unit(UnitKind::Warrior, Faction::Red, 15, 5, false);
    game.player_step(SwipeDir::E);
    // Should have player move + AI move
    let move_count = game.turn_events.iter().filter(|e| matches!(e, TurnEvent::Move { .. })).count();
    assert!(move_count >= 2, "Expected at least 2 Move events (player + AI), got {move_count}");
    // Drain (simulating game_loop behavior)
    let events: Vec<_> = game.turn_events.drain(..).collect();
    assert!(game.turn_events.is_empty());
    assert!(events.len() >= 2);
}
```

- [ ] **Step 2: Run test**

Run: `cargo test game::tests::turn_events_accumulate_player_and_ai -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/game.rs
git commit -m "test(animation): verify turn events accumulate and drain correctly"
```

---

### Task 14: Final integration build and WASM verification

**Files:** None (verification only)

- [ ] **Step 1: Run all tests**

Run: `cargo test 2>&1 | tail -10`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | tail -10`
Expected: no warnings

- [ ] **Step 3: Build WASM**

Run: `wasm-pack build --target web 2>&1 | tail -5`
Expected: clean build

- [ ] **Step 4: Final commit (if any clippy fixes needed)**

```bash
git add -A
git commit -m "chore(animation): fix clippy warnings"
```

- [ ] **Step 5: Verify in browser**

Open the game in browser. Verify:
- Units slide smoothly between tiles instead of teleporting
- AI units move with stagger (not all at once)
- Attack animations play with particles
- Camera smoothly tracks player movement
- Input is blocked during animation playback
- Auto-move works with animation
