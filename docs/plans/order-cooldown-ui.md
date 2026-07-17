# Arcade Controls: Auto-Follow Retinue, Per-NPC Order Cooldown, Dismiss

North star: **the game fits an arcade cabinet** — one joystick + a standard
4-button layout, arcade pacing:

```
JOYSTICK        move (the primary skill verb)
BUTTON 1        ATTACK   — manual cone attack (unchanged)
BUTTON 2        CHARGE   — send your retinue at a point ahead of you
BUTTON 3        DEFEND   — post your retinue in a line at your position
BUTTON 4        DISMISS  — release your retinue back to the army (hold)
```

This replaces the previous global-cooldown + 3-order-button plan.
Auto-attack was weighed and **deferred** — the 4-button cabinet layout has
room for a manual attack button, so it stays; see Deferred at the end.

## Design

### Auto-follow — the retinue

The Follow *order* is removed. Instead, soldiers join you on their own:

- **Recruitment tick** (~1s cadence): each allied unit inside your command
  radius with no active order and no commitment cooldown rolls the existing
  deterministic acceptance check (same hash, same authority bracket). Success
  → the unit becomes a **follower**: `OrderKind::Follow`, **sticky** (no
  expiry timer), capped by `authority_max_followers()`.
- Failures are silent (no reject flash) — passive recruitment shouldn't spam
  "?" markers; the deterministic roll means the same unit keeps refusing
  until your authority bracket changes.
- **The vacuum is a feature**: walking near a defended zone pulls its
  defenders into your wake. Where you walk is a tactical decision — your
  authority is a magnet you cannot turn off, only manage (walk around your
  own lines, or Dismiss).
- Followers are released by: **Dismiss** (all at once), follower death,
  player death, or losing contact (further than a leash distance for a few
  seconds — stragglers return to army AI instead of pathing across the map).

Authority progression now reads on screen with zero UI: at Unknown you walk
alone; at Legend a warband forms around you as you cross the field.

### Orders — Charge / Defend, retinue only

Charge and Defend now command **your followers**, not everyone in radius:

- No acceptance roll at order time — these soldiers already chose you.
  Recruitment is where authority gates; ordering your own retinue always
  works (subject to per-NPC commitment, below).
- Charge keeps its current behavior (rush a point ahead of your aim, then
  auto-transition back to Follow). Defend keeps the layered line at your
  position/facing; on expiry units revert to Follow and return to you.
  Followers stay "yours" through the whole Follow ↔ Charge/Defend cycle.
  **Lend vs give**: Defend lends your soldiers to a position; Dismiss gives
  them back to the army.

### Dismiss

A **hold** (~0.4s with a fill ring on the button — misprint protection for a
warband-losing action; also positioned farthest from the joystick), releasing
the **entire retinue** at once. No partial dismissal — arcade format forbids
unit selection.

- Released units immediately clear their order and marker (even
  mid-commitment — you can always let people go) and return to the faction
  planner, which reassigns them to nearby objectives on its next scoring
  pass. **Troop ferrying** falls out for free: walk a retinue to a contested
  zone, dismiss there, and you've delivered a garrison — a strategic verb
  made of pure movement.
- **Re-recruit immunity** (the detail that makes Dismiss work at all):
  dismissal sets a per-unit `re_recruit_cooldown` — otherwise the next
  recruitment tick re-rolls the same deterministic "yes" and the warband
  re-attaches while you're still standing there. Duration is dictated by
  geometry: at least the time to walk out of your own command radius
  (~12 tiles at Legend) → default **12s**, config knob, per-unit (dismiss
  squad A at a zone, recruit fresh units elsewhere immediately).
  - Known fallback if the timer feels wrong in playtests: radius-exit-based
    immunity (immune until you leave and re-enter their vicinity). Timer
    ships first — simpler and testable.
- **No authority cost** — punishing the release valve teaches players never
  to use it, and the vacuum then has no counterplay.
- Feedback: "dismissed!" float at the player, a salute-tint order flash on
  released units, follower counter drops; grey "no one to dismiss" float on
  empty press.

### Action-timing commitment (per NPC)

Per the design constraint: commitment is **per NPC** and **tied to action
timing** — never global, never per-order-type, never a flat timer.

A unit is committed exactly while it is *physically executing* the transient
part of an action, and re-taskable the moment it reaches a steady state.
Derived (`Unit::is_committed()`), not stored:

| State | Committed | Until |
|-------|-----------|-------|
| Charging toward the target | yes | arrival (existing auto-transition to Follow) or the existing charge timeout |
| Walking into a Defend slot | yes | in position at the slot |
| Posted in the Defend line | no | — |
| Following the player | no | — |
| Mid-attack swing | yes | swing ends (existing `can_act`) |

- Committed units ignore re-tasking and recruitment; steady-state units obey
  instantly. "Order squad A, run over, order squad B" flows freely because
  each unit derives its own state.
- Flip-flop micro is priced diegetically: re-charging after arrival is a new
  action whose cost is the real travel time — no artificial lockout needed.
- Dismiss cuts through commitment (you can always let people go; see
  Dismiss above — its re-recruit immunity is the separate
  `re_recruit_cooldown`).
- UI: the existing per-unit order marker's progress bar shows the action in
  progress — now an honest signal, not an invented cooldown. Buttons are
  never disabled or arc-swept.
- No config knob: pacing derives from movement speed and distances already
  in the game.

## Consolidation (unchanged in spirit from the previous plan)

- **Typed order model**: `PlayerInput` drops `recruit`, `order_follow`,
  `order_charge`, `order_defend`; gains
  `pub order: Option<OrderRequest>` with
  `enum OrderRequest { Charge, Defend, Dismiss }`. `issue_order` takes the
  enum (strings gone) and returns an outcome (`Issued(n)` /
  `NoFollowers` / `NoPlayer`).
- **Presentation constants on the type**: `label()`, `short_label()`
  (C / D / X), `color()` — defined once in core so SDL and wgpu cannot
  drift.
- **`TouchControls` in core** (`battlefield_core::touch_input`): joystick +
  the order buttons, the layout math, touch routing, and edge-detected
  `take_order()` — deleting the duplicated fields/layout/routing in
  `crates/sdl/src/input.rs` and `crates/wgpu/src/input.rs`. Platform files
  shrink to raw-event translation (SDL events / winit) + keyboard/gamepad
  mapping onto the same `Option<OrderRequest>`.
- **Command-radius pulse moves to core** (`Game.order_pulse`), so SDL gains
  the pulse currently only in wgpu.
- **Feedback**: on `Issued(n)`, floating text "n soldiers!" at the player via
  the existing `floating_texts` pipeline (both renderers free). New: a small
  follower count near the authority bar (e.g. "5/8") — your retinue size vs
  cap is now a core stat.

## Attack alignment (AI ↔ player)

The player must fight exactly like other soldiers. Audit findings
(2026-07-17): three asymmetries — the player cleaves all enemies in a 180°
cone with knockback but reaches only 1.0 tile; AI melee hits a single target,
no knockback, but reaches 1.5 tiles (`MELEE_RANGE`).

Decision: **align AI to the player's swing, and unify reach upward.**

- AI melee (Warrior, Lancer) swings become cone attacks: all enemies in the
  180° arc toward the primary target are hit, each with knockback.
- Both player and AI melee reach = `MELEE_RANGE` (1.5 tiles) — aligns the
  player up instead of touching every AI combat path.
- Archer (projectile) and Monk (heal) are unchanged.
- **Balance consequence**: cleave in AI-vs-AI blobs accelerates kills
  battle-wide — rerun the manpower probe seeds afterward; `manpower_start` /
  `bleed_per_extra_zone` may need retuning.

## Roadmap (checkable)

Status legend: item is checked when its tests pass and both renderers build.

- [x] **0a. Attack-rate parity** — hold/spam = AI rate (verified: held input
  + `can_act` gate); whiff-cooldown bug fixed to use config (`player.rs`).
- [x] **0b. Sticky Follow + auto-recruitment** — 1s pass, radius + cap +
  deterministic roll, silent rejection, `re_recruit_cooldown` respected,
  lost-contact release (15 tiles / 3s), release on player death, timed
  orders revert to Follow. *(9 tests; uncommitted)*
  - Note: rally-hold units at base ARE recruitable (wave-vacuum = ferry
    play); lost-contact release sets NO re-recruit immunity (stragglers may
    rejoin). Flag if either should change.
- [x] **1. Attack alignment** — AI melee cone cleave + knockback; unified
  `MELEE_RANGE` reach for player (`swing_in_cone` / `attack_target` in
  combat.rs). Knockback default retuned 0.7 → 0.2 tiles: constant AI
  knockback at 0.7 shoved fronts apart faster than damage accrued and
  battles stopped killing.
- [x] **2. Action-timing commitment** — derived `Unit::is_committed()`
  (Charge in flight; Defend walking to slot via `defend_in_position`).
- [x] **3. Typed orders + Dismiss** — `OrderRequest`/`OrderOutcome`,
  retinue-only targeting, `PlayerInput.order`, Follow order path +
  `recruit` field + stringly API removed; Dismiss releases all with 12s
  `re_recruit_cooldown`; `reject_flash` removed (nothing rejects anymore).
- [x] **4. `TouchControls` in core** — full touch stack (joystick, buttons,
  camera drag, two-finger gestures) shared; both renderer `input.rs` files
  reduced to platform-event translation; dismiss hold at 0.4s.
- [x] **5. Drawing + HUD** — buttons from `OrderRequest` accessors, hold
  ring, radius pulse moved to core (SDL gains it), follower counter next
  to the authority bar. Acknowledgement floats dropped: `FloatingText` is
  numeric-only; the counter + pulse carry the feedback.
- [x] **6. Verify + rebalance** — fmt/clippy/tests green (186); headless
  probes: 3 of 4 seeds end in 5–14 min via annihilation or sudden death.
  **New rule added during verification**: sudden death — with both pools
  exhausted, a strict zone majority held 60s wins (probe found a 35v35
  double-exhaustion standoff that annihilation could not break).

## Stall investigation (resolved)

The probe stalls traced to four distinct causes, all fixed:

1. **Nondeterministic simulation** — `resolve_collisions` iterated a
   `HashMap` (random order per process), so identical seeds produced
   different battles. Cells are now iterated in sorted order; same seed =
   identical battle, byte for byte.
2. **Monk accumulation** — fighters died and were replaced by cycling the
   WAVE pattern while fleeing monks never died; losing armies curdled
   into heal-stacks (11 monks of 30) that out-healed all damage.
   Production is now deficit-based (`build_wave`): each slot goes to the
   kind furthest below its WAVE-share of the army.
3. **Sudden-death timer flicker** — a stable 4–3 zone majority never held
   60 *unbroken* seconds because frontline zones dip to contested for
   moments. In sudden death, ties/flicker now pause the timer; only an
   actual leadership flip resets it. (Domination keeps strict resets.)
4. **Fractional pool never exhausts** — bleed could leave a pool at e.g.
   0.7: unable to field a unit (spawn costs 1.0) yet not `<= 0`, so no
   end condition armed. Exhaustion/annihilation thresholds are now
   `< 1.0`.

Probe results after fixes (deterministic): seeds 42/1234/21 end 354–393s
by annihilation; 99 at 462s; 5 at 722s by sudden-death majority (16
soldiers beating 35 on zone standing); 7 at 1921s by sudden death. The
bench prints composition + zone standing for future probes.

## Follow-up

- **Seed 777 performance pathology**: healthy battle state but ~3.2ms
  avg frame vs ~0.3ms on other seeds (10x). Within 60fps budget on
  desktop but borderline on Pi-class hardware. Likely terrain-driven
  (map has a zone that stays neutral at 300s — suspect failed-path A*
  churn or flow-field recompute on awkward topology). Needs its own
  profiling pass (`BENCH_SEED=777`, criterion `game_tick`).

## Decisions taken

- **Vacuum stays** — pulling your own defenders off zones by walking past is
  intended tactical texture, not a bug to fence off.
- **Recruitment is where authority gates; retinue orders always obey.**
- **Commitment = action timing** — derived from what the unit is physically
  doing (charge in flight, walking into line), no stored timer, no knob;
  steady states (following, posted) are always re-taskable.
- **Manual attack stays** — standard 4-button cabinet layout has room;
  auto-attack deferred (below).
- **Dismiss is a hold, all-or-nothing, no authority cost, 12s per-unit
  re-recruit immunity** (timer-based; radius-exit rule is the fallback if
  playtests show re-attach annoyance).
- **Keyboard**: Attack on Space (unchanged); Charge / Defend / Dismiss on
  J / K / L.

## Deferred

- **Auto-attack** — player attacking automatically like every other soldier
  (consistent with the ordinary-soldier pillar) was weighed and deferred:
  the 4-button layout keeps a manual attack button. Revisit if cabinet
  playtests show attack-mashing fatigue; the change is small (player tick
  fires `player_attack()` when an enemy is in cone, behind a config bool).

## Out of scope

- New order types, radial menus, per-unit selection.
- Scoring/attract-mode and other cabinet trappings (own plan later).
- Morale, manpower rebalance beyond the verification probe.
