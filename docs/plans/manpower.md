# Manpower / Conquest Plan

Battlefield-Conquest-style attrition so battles structurally end, instead of
oscillating forever between two infinite armies.

## Design

### Manpower pool

Each faction starts the battle with a finite **manpower pool** (default: 100).
Manpower is the number of reinforcements a faction can still field.

- Spawning one reinforcement unit costs **1 manpower**.
- The starting army (the initial WAVE around each base + the player) is
  **free** — it does not draw from the pool, so the opening plays exactly as
  it does today.
- When the pool is empty, production stops. The faction fights on with
  whatever units it has left.

### Zone bleed (the Conquest mechanic)

Holding a **majority of zones (4+ of 7)** drains the enemy pool over time:

```
bleed/sec = (zones_controlled - 3) * bleed_per_extra_zone   when zones_controlled >= 4
```

Default `bleed_per_extra_zone = 0.5/s`. Examples: holding 4 zones drains the
enemy 0.5/s; holding all 7 drains 2.0/s (a full pool gone in ~50s of total
domination). Pool is stored as `f32` so fractional bleed accumulates; spawns
cost `1.0`.

This makes zone control matter *continuously*, not only at the all-7 victory
threshold, and guarantees convergence: any sustained majority ends the battle
eventually.

### Battle end conditions

1. **Domination** (existing, unchanged): hold all 7 zones for 60s → instant win.
2. **Annihilation** (new): a faction with `manpower <= 0` **and no living
   units** loses; the opposing faction is set as `winner`.
3. **Player death** (existing, unchanged): permadeath ends the run regardless.

Both end conditions flow through the existing `Game.winner` field, which the
game loops already map to `GameWon` / `GameLost` — no new screen plumbing.

### Interactions with existing systems

- **Desperation double waves** (holding 0 zones) stay as-is. They now burn the
  pool twice as fast — a genuine gamble: an all-in comeback push that shortens
  the battle if it fails. This is intentional; no tuning change.
- **Skip-rally when dominating** stays as-is.
- **Wave sizing**: a wave is capped by remaining manpower in addition to the
  existing unit-cap logic (`floor(manpower)` at queue build, re-checked at
  each spawn since bleed can drain the pool mid-wave).
- The player's kills gain strategic weight for free: every enemy killed is a
  manpower point the enemy must spend to replace.

### HUD

Two manpower counters (Blue / Red) in faction colors, near the top of the
screen — the Battlefield ticket-counter idiom. Show a subtle drain indicator
on a pool that is currently bleeding. Displayed in both renderers.

## Implementation Steps

Repo convention is TDD — each step starts with its failing test in
`crates/core` (headless, no renderer needed).

### 1. Config knobs — `crates/core/src/config.rs`

```rust
// ── Manpower / Conquest ─────────────────────────────────────────
pub manpower_start: f32,          // default 100.0
pub bleed_zone_threshold: usize,  // default 4 (majority of 7)
pub bleed_per_extra_zone: f32,    // default 0.5 per second
```

### 2. Game state — `crates/core/src/game/mod.rs`

- Add `pub manpower: [f32; 2]` to `Game` (index 0 = Blue, 1 = Red, matching
  the existing `spawn_queue` / `macro_objectives` convention).
- Initialize both to `config.manpower_start` in `Game::new`.

### 3. Production caps — `crates/core/src/game/setup.rs::tick_production`

- When building a wave queue: `wave_size = slots.min(max_wave).min(manpower[fi] as usize)`.
- At each spawn tick: if `manpower[fi] < 1.0`, clear the queue (release any
  rallying units so a partial wave still marches) and skip; otherwise
  decrement `manpower[fi] -= 1.0` alongside `spawn_unit`.

Tests: wave queue capped by pool; pool decrements per reinforcement spawn;
production stops at 0; partial wave releases its rally hold; starting armies
spawn with a full pool untouched.

### 4. Bleed + annihilation — `crates/core/src/game/setup.rs::tick_zones`

After zone states update (same place `tick_victory` runs):

- Count `Controlled(faction)` zones per faction; for each faction at or above
  `bleed_zone_threshold`, drain the *enemy* pool by
  `(controlled - (threshold - 1)) * bleed_per_extra_zone * dt`, clamped at 0.
- Annihilation check (only when `winner.is_none()`): if a faction has
  `manpower <= 0.0` and no `alive` units, set `winner = Some(enemy)`.

Tests: bleed drains at the expected rate at 4 and 7 zones held; no bleed below
threshold; annihilation sets winner only when pool empty AND army dead;
domination victory still fires independently.

### 5. HUD counters

- `crates/sdl/src/renderer/hud.rs` — draw `⚑ 87` / `⚑ 62` style counters in
  faction colors, top-center (clear of the top-right minimap).
- `crates/wgpu/src/renderer/mod.rs` (HUD section) — same layout via the text
  batch.
- Tint or pulse a counter red while that pool is actively bleeding.

### 6. Balance pass

**Done — measured with `bench-headless` (`BENCH_RUN_TO_END=1`, plus
`BENCH_MANPOWER`/`BENCH_BLEED` overrides added for tuning probes):**

- The original guess (`manpower_start = 100`, bleed 0.5/s/zone) ended AI-vs-AI
  battles in ~3 minutes — bleed dominated the drain.
- Shipped defaults: `manpower_start = 300`, threshold 4, bleed 0.25/s/zone →
  6–8 minute AI-vs-AI battles across seeds, ending via both annihilation and
  domination. All three remain config values for further tuning.

## Explicitly out of scope

- Per-death ticket cost (Battlefield charges tickets on death; here the
  replacement-spawn cost is equivalent in steady state and simpler).
- Manpower pickups / zone-based production boosts.
- Showing enemy manpower only when scouted (fog-of-war on the counter).
- Authority/reputation hooks into manpower events.
