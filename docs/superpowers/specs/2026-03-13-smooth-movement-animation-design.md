# Smooth Movement Animation System

## Problem

Units teleport between tiles instantly. Player acts, AI acts, turn advances — all in a single frame. No visual feedback of movement, making the game feel mechanical.

## Solution

An **animation queue** system that records turn events during game logic execution, then plays them back as smooth interpolated movement. Game state resolves instantly (grid positions update); rendering interpolates visual positions.

## Architecture

### Turn Events

`player_step()` and `ai_turn()` push events to `Game::turn_events: Vec<TurnEvent>` as they execute:

```rust
pub enum TurnEvent {
    Move { unit_id: UnitId, from: (u32, u32), to: (u32, u32) },
    MeleeAttack { attacker_id: UnitId, defender_id: UnitId, damage: i32, killed: bool },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
        target_pos: (u32, u32),
        missed: bool,
    },
}
```

Game logic is unchanged — `grid_x`/`grid_y` update instantly. Events are a passive record. Death is not a separate event — it is tracked via the `killed` flag on attack events and `unit.alive == false`.

### Visual Position Layer

Each `Unit` gains two fields:

```rust
pub visual_x: f32,  // world-space pixel X
pub visual_y: f32,  // world-space pixel Y
```

**Initialization:** `Unit::new()` computes `visual_x/y` from `grid_to_world(grid_x, grid_y)`. Any code that mutates `grid_x/grid_y` outside of animation (e.g., `spawn_unit()` during `setup_demo_battle`) must also sync `visual_x/y`.

**Rendering:** The renderer draws at `(visual_x, visual_y)` instead of `grid_to_world(grid_x, grid_y)`. When no animation is active, `visual_x/y` snaps to `grid_to_world(grid_x, grid_y)` each frame (in case grid position was changed outside the animation system). During animation, `visual_x/y` is driven by the animator.

**Dead units during animation:** A unit whose `alive == false` (killed during instant game logic resolution) must still render normally during animation phases that precede its death. The animator tracks a `visual_alive: HashSet<UnitId>` that starts with all alive units at the beginning of playback. When a `MeleeAttack` or `RangedAttack` phase with `killed: true` completes, that unit is removed from `visual_alive` and its `death_fade` timer starts. The renderer checks `visual_alive` (when animator is playing) instead of `unit.alive` for visibility.

### Animation Phases

A new `TurnAnimator` (in `src/animation.rs`) converts events into sequenced phases:

```rust
pub struct TurnAnimator {
    phases: VecDeque<AnimPhase>,
    phase_elapsed: f32,
    /// Units visually alive during playback (may differ from unit.alive).
    visual_alive: HashSet<UnitId>,
}

pub enum AnimPhase {
    /// Multiple units move simultaneously with staggered start times.
    ParallelMoves {
        moves: Vec<MoveAnim>,
        total_duration: f32,  // = max(delay + duration) across all moves
    },
    /// Melee attack: attacker plays Attack anim, defender takes damage.
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
        duration: f32,
    },
    /// Ranged attack: arrow flies from attacker to target position, then hit/miss.
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        target_pos: (f32, f32),
        missed: bool,
        damage: i32,
        killed: bool,
        duration: f32,
    },
}

pub struct MoveAnim {
    pub unit_id: UnitId,
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub delay: f32,
    pub duration: f32,
}
```

### TurnAnimator API

```rust
impl TurnAnimator {
    /// Feed turn events and compile into animation phases.
    /// `is_auto_move` controls movement duration (fast vs normal).
    pub fn enqueue(&mut self, events: Vec<TurnEvent>, is_auto_move: bool);

    /// Advance animation. Takes mutable access to units to update
    /// visual_x/y and set_anim(). Returns particles/projectiles to spawn.
    pub fn update(&mut self, dt: f32, units: &mut [Unit]) -> AnimOutput;

    /// True while phases remain.
    pub fn is_playing(&self) -> bool;

    /// Check if a unit should be rendered (visually alive during playback).
    pub fn is_visually_alive(&self, unit_id: UnitId) -> bool;
}

pub struct AnimOutput {
    pub particles: Vec<(ParticleKind, f32, f32)>,
    pub projectiles: Vec<(f32, f32, f32, f32)>,  // (sx, sy, tx, ty)
}
```

The `update()` method takes `&mut [Unit]` directly, allowing it to set `visual_x/y` and call `set_anim()`. It returns any particles/projectiles to spawn, which the game loop pushes into `Game::particles` and `Game::projectiles`.

### Event-to-Phase Compilation

After `player_step()` returns, `game.turn_events.drain(..)` is compiled into phases by `animator.enqueue()`:

1. **Phase: ParallelMoves** — Collect all `Move` events. Player move gets delay 0.0s; AI moves get staggered delays (0.05s * index). Duration: `MOVE_DURATION_AUTO` (0.15s) if `is_auto_move`, else `MOVE_DURATION_SINGLE` (0.25s). `total_duration = max(delay + duration)` across all moves. Dust particles are spawned at each move's `from` position when the move starts (delay reached).
2. **Phase: MeleeAttack(s)** — One phase per `MeleeAttack` event, sequenced.
3. **Phase: RangedAttack(s)** — One phase per `RangedAttack` event (arrow flight + hit/miss).

If a phase has `killed: true`, on phase completion the unit is removed from `visual_alive` and `death_fade` is triggered.

### Playback

`TurnAnimator::update(dt, units)` advances the current phase:

- **ParallelMoves**: For each move, compute `t = clamp((elapsed - delay) / duration, 0, 1)`. Set `unit.visual_x/y = lerp(from, to, ease_out(t))`. Set unit anim to `Run` while `0 < t < 1`, `Idle` otherwise. Phase completes when `elapsed >= total_duration`.
- **MeleeAttack**: Set attacker anim to `Attack`. At 60% through duration, emit damage particle at defender position. Phase completes at `duration`. If `killed`, remove from `visual_alive`.
- **RangedAttack**: Emit projectile at phase start. On completion, emit hit/miss particle. If `killed`, remove from `visual_alive`.

When all phases are drained, `is_playing()` returns false. At this point, all units snap `visual_x/y` to `grid_to_world(grid_x, grid_y)` and `visual_alive` is cleared.

### Input Blocking

While `animator.is_playing()`:
- Swipe, click, arrow key input is ignored (not consumed — just skipped).
- Auto-move timer pauses (time-based, not frame-based — see below).
- Camera follows **player's `visual_x/visual_y`** for smooth tracking. If the player is dead, camera holds at last known visual position until animation completes.

### Auto-Move Timer

Convert the existing frame-based auto-move timer (`auto_move_timer += 1; if >= 8`) to time-based: accumulate `dt`, fire at 0.15s threshold. This aligns with `MOVE_DURATION_AUTO` and works correctly at variable frame rates.

Each `auto_move_step()` call produces one event batch via `player_step()`. The `repath_around_units()` case (which calls `player_step()` internally) produces a single merged batch because it happens within the same `auto_move_step()` call — `turn_events` accumulates across both calls.

### Particle/Projectile Ownership

All particle and projectile spawning moves from `game.rs` to the animator:

**Removed from game.rs:**
- Dust particles in `player_step()` (movement dust)
- Dust particles in `ai_turn()` (AI movement dust)
- All particles in `execute_attack()` (ExplosionSmall, ExplosionLarge, Dust for ranged miss)
- Projectile spawning in `execute_attack()` (arrow creation)

**Spawned by animator via `AnimOutput`:**
- Dust at `from` position when a `MoveAnim` starts
- ExplosionSmall at defender position at 60% of `MeleeAttack`
- ExplosionLarge at defender position when `killed` on phase completion
- Dust at `target_pos` on `RangedAttack` miss
- Arrow projectile at `RangedAttack` phase start

### Camera Follow

`Game::update()` camera tracking switches from `grid_to_world(player.grid_x, player.grid_y)` to `(player.visual_x, player.visual_y)`. This ensures the camera smoothly follows the player during movement animation instead of snapping ahead.

### Easing

`ease_out_quad(t) = 1 - (1 - t)^2` for movement. Natural deceleration feel.

## Files Changed

| File | Change |
|------|--------|
| `src/animation.rs` | **New** — `TurnEvent`, `AnimPhase`, `MoveAnim`, `TurnAnimator`, `AnimOutput`, easing functions |
| `src/unit.rs` | Add `visual_x: f32`, `visual_y: f32` to `Unit`. Initialize in `Unit::new()` via `grid_to_world()`. |
| `src/game.rs` | Add `turn_events: Vec<TurnEvent>`. Push events in `player_step()`, `ai_turn()`, `execute_attack()`. Remove all particle/projectile spawning from these methods. Convert `auto_move_timer` to `f32` time-based. Camera follow uses `visual_x/y`. |
| `src/game_loop.rs` | Create `TurnAnimator`. After input processing, compile events via `animator.enqueue()`. Each frame: call `animator.update(dt, &mut units)`, push returned particles/projectiles to game. Block input while playing. Renderer uses `visual_x/y`. Use `animator.is_visually_alive()` for unit rendering during playback. |
| `src/lib.rs` | Add `pub mod animation;` |

## Timing Constants

| Constant | Value | Notes |
|----------|-------|-------|
| `MOVE_DURATION_SINGLE` | 0.25s | Single step or AI move |
| `MOVE_DURATION_AUTO` | 0.15s | Auto-move steps (faster to avoid sluggishness) |
| `AI_STAGGER_DELAY` | 0.05s | Delay between each AI unit starting movement |
| `MELEE_ATTACK_DURATION` | 0.35s | Melee attack animation phase |
| `RANGED_ATTACK_DURATION` | 0.5s | Arrow flight + impact |
| `DEATH_FADE_DURATION` | 0.3s | Existing constant, unchanged |

## What Does NOT Change

- Game logic (turn resolution, combat math, pathfinding, FOV)
- Grid state model (`grid_x`/`grid_y` remain the source of truth)
- Particle/projectile rendering (unchanged, just spawned by animator instead of game logic)
- Auto-move queue logic (pathfinding, repath)
- Fog of war computation (still triggers on `grid_x/y` change in `player_step()`)

## Edge Cases

- **Unit dies mid-turn**: Game state marks `alive = false` instantly. Animator's `visual_alive` set keeps the unit rendered until its death phase completes. The death phase removes it from `visual_alive` and triggers `death_fade`.
- **Player dies during AI turn**: Camera holds at player's last `visual_x/visual_y` until all phases complete. Game over check deferred until animation finishes.
- **Auto-move rapid fire**: Each `auto_move_step()` produces one event batch. The time-based auto-move timer (0.15s) aligns with `MOVE_DURATION_AUTO` (0.15s). If animator is still playing when timer fires, the step waits.
- **Repath during auto-move**: `repath_around_units()` calls `player_step()` internally. Both calls accumulate into the same `turn_events` vec, producing a single merged batch.
- **No movement turn** (player attacks adjacent enemy): No `ParallelMoves` phase — goes straight to `MeleeAttack`.
- **Multiple AI attacks in one turn**: Each attack gets its own sequential phase. Ranged attacks after melee.
- **Tap to skip (future)**: Not in initial implementation. Can be added later by snapping all `visual_x/y` to grid positions and draining all phases.
