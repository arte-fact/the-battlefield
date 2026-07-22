# Conquest Economy — Victory, Production & Score Rework

Replace the pre-network attrition stack (manpower pools, leader bleed,
domination timer, sudden death, stagnation patch, garrison sub-system,
central waves) with one loop that matches what the game has become:

> Settlements collect resources. Resources train soldiers, at the
> buildings that own them, everywhere at once. A faction dies when it
> has nothing left to fight with. The last banner standing wins.

Decisions taken with the designer (2026-07-22):

- Manpower pools and leader bleed **removed**; replaced by a per-faction
  war score display and pure annihilation.
- Reinforcement spawning is **fed by resources collected** by owned
  settlements' peons.
- The garrison sub-system is **removed**: all owned production
  buildings spawn **normal soldiers, equally**.
- Victory is **conquest only** (last banner standing).
- Arcade score counts **deeds only** (no survival trickle).
- Per-mode flags stay (free mode keeps its own variant hook).

---

## 1. The resource economy (one rule for every settlement)

- Peon deliveries keep banking **stock** per settlement
  (`village_stock`, cap unchanged) — this now applies to **capitals
  too**: cities already have worked resource rings; their deliveries
  bank stock the same way. Stock is the only war currency.
- Every **production building** in a settlement owned by a faction
  spends 1 stock to train 1 **normal soldier** of its production kind,
  on a fixed interval (`train_interval`, default ≈ the old garrison
  6 s, tuned so total output roughly matches today's wave flow at
  equal holdings). All owned buildings train **equally and locally**:
  a 12-settlement empire reinforces from 12 places at once.
- Trained soldiers are ordinary army units: they join the faction
  planner's objectives from wherever they were born (hierarchical
  navigation already handles cross-map marches). No waves, no rally
  phase, no comeback double-waves, no spawn queue.
- The **army cap** (`max_units_per_faction`) remains the throttle on
  total force; buildings pause training at the cap (stock keeps
  banking up to its cap — a reserve for after the next battle).
- **Neutral settlements** run the same economy in Villager colors:
  their buildings train Black militia who stay home (no planner) and
  fight anyone who comes close — villages still defend themselves,
  without a dedicated garrison mechanic. Surviving militia still
  pledges to a captor. Militia sleep keys on Villager-faction units
  near quiet settlements instead of the DefendZone order.
- **Removed**: `manpower[]`, spawn cost accounting, wave batching,
  `tick_manpower_bleed`, `bleed_*` config, `garrison_cap`,
  `tick_village_garrisons` (folded into the one training rule),
  `convert_garrison` (militia pledge moves to capture handling).
- **Kept**: the player's long-press Defend stationing (`DefendZone`
  order) — that is a player command, not the garrison system — and
  garrison-style recruitment of stationed soldiers back into the
  retinue.

## 2. Victory: conquest only

- A faction is **eliminated** when it owns **no settlements** and has
  **no living units**. (No settlements + a wandering remnant army =
  still alive; it can recapture. No income means it withers.)
- **Winner** = the last army faction standing. The player's run ends
  in victory/defeat when their faction wins / is eliminated, and
  immediately on player death as today.
- **Removed**: domination hold timer + victory progress bar, sudden
  death, the stagnation detector, `victory_hold_time` gameplay use,
  `tick_victory`/`tick_victory_majority`, `zone_manager` victory
  timers.
- Free mode: conquest rules are naturally untimed; the mode flag
  (`Game::untimed`) stays as the per-mode hook (currently a no-op)
  per the designer's choice.
- **Accepted risk**: with no clock, a perfectly balanced AI-vs-AI
  stalemate can run long. Positional economy makes them self-breaking
  in practice (each captured village compounds); probes will measure
  endgame length across the seed matrix, and a mercy rule can be
  added later behind the mode flag if data demands it.

## 3. War score (the HUD number that replaces manpower)

- Per-faction **war score** = tier-weighted settlements held
  (City 4, Town 3, Village 2, Hamlet 1). It is a *display and AI
  signal*, not a resource: the HUD row under the settlement strip
  shows each faction's score in its color; the planner's
  settlement-leader weighting reads the same number (replacing
  "most zones" as the gang-up signal).
- Elimination/endgame resolution needs no score — conquest is binary.

## 4. Arcade score: deeds only

`kills × 100 + captures × 500 + peak_authority × 10 + victory bonus
5000`, all `× enemy count`. The survival-seconds term is removed — the
score measures what the soldier did, not how long they hid. Scoreboard,
initials, and the ladder are unchanged.

## 5. Skirmish rows

- **Removed rows**: MANPOWER (YOU), MANPOWER (ENEMY), VICTORY HOLD,
  ZONE BLEED.
- **New row**: PRODUCTION — SLOW / NORMAL / FAST (global
  `train_interval` multiplier ×1.5 / ×1.0 / ×0.6). One knob for war
  pace instead of four attrition knobs.
- Kept: MAP SEED, ENEMIES, MAP SIZE, START AS, ARMY SIZE CAP,
  STARTING AUTHORITY. (7 rows total; panel shrinks back.)

## 6. Touch points

- `game/setup.rs`: production/waves/bleed/sudden-death/annihilation
  rewrite; `zone.rs`: drop victory timers, add tier weights for war
  score; `config.rs`: remove manpower/bleed/hold fields, add
  `train_interval` + multiplier; `ui.rs`: rows, score formula;
  HUD (both renderers): war-score row replaces manpower counters,
  victory-progress bar removed; `ai.rs`: planner leader weight reads
  war score; militia-sleep filter keys on faction; bench-headless:
  report war scores instead of pools.

## 7. Implementation order

1. **Economy core**: stock-fed training at all owned buildings
   (capitals included), waves/manpower removed, militia via the same
   rule; probes must still end on the default matrix.
2. **Conquest victory**: elimination rule, all timers deleted,
   free-mode flag becomes a no-op hook.
3. **War score**: zone tier weights, HUD rows, planner signal,
   bench output.
4. **Arcade score + skirmish rows**; GDD rewrite of the three
   sections; screenshot refresh.
5. **Verify**: full seed matrix 1v1/1v3 at AUTO + 512², endgame
   length distribution, browser run of all three modes.

## Open questions (deferred, defaults chosen)

- Training pace default: tuned so a 5-settlement empire fields about
  one soldier per 2 s — close to today's aggregate wave output;
  adjust after probes.
- Does capturing a settlement seize its banked stock? Default **yes**
  (loot), it makes pushes rewarding and is one line.
- Starting armies: unchanged (free, spawned at the capital).
- Comeback mechanic (old double waves): none in v1 — the gang-up
  planner weight already pressures the leader.
