# Pawn Economy — People Are the Production

Replace the abstract training timer with a fully diegetic pipeline:

> Houses raise pawns. Pawns walk into a production building and come
> out as soldiers. Meat grows the population; gold arms warriors and
> monks; wood arms archers and lancers.

Every reinforcement becomes visible on the field — the same journey
the free-mode player makes (villager walks into a barracks, soldier
walks out) becomes the universal rule of the world.

Decisions taken with the designer (2026-07-22):

- **Faction-wide typed pools**: meat / gold / wood per faction;
  deliveries anywhere feed them, any owned building spends them.
- **Workers fixed, recruits extra**: worker pawns stay bound to houses
  as today; houses additionally spawn dedicated recruit pawns.
- **All pawns invulnerable**: recruits flee combat like workers and
  always arrive eventually.
- **Villager network shares**: all neutral settlements pool their
  resources as one Villager economy ("the free villages trade").

---

## 1. Typed resources

- Three currencies: **MEAT, GOLD, WOOD**. Per-faction pools, including
  a fifth pool for the Villager network: `resources: [[u32; 3]; 5]`.
- A peon delivery adds 1 of its settlement's **theme resource** to the
  **owner faction's** pool (neutral settlements feed the Villager
  pool). `village_stock` and its per-zone cap are deleted.
- Pool cap 99 per type (display-friendly; effectively soft).
- Starting pools per faction: 5 meat / 3 gold / 3 wood so production
  starts immediately.
- Capturing settlements doesn't loot pools (they're faction-wide);
  what you seize is the **income and the housing**.

## 2. Costs

| Thing | Cost | Why |
|-------|------|-----|
| Recruit pawn (house spawn) | 1 MEAT | food grows people |
| Warrior conversion | 1 GOLD | armor and steel |
| Monk conversion | 1 GOLD | the church takes coin |
| Archer conversion | 1 WOOD | bows |
| Lancer conversion | 1 WOOD | lances |

Pastures are population engines every faction needs; gold and wood
territory shapes what your army looks like. Composition is written on
the map.

## 3. The recruit pipeline

- Each **house** in an owned settlement runs a spawn timer
  (`recruit_interval`, scaled by the skirmish PRODUCTION row). On
  fire, if the faction has 1 meat, is under the army cap, and the
  settlement has a production building whose typed cost is currently
  affordable, the house spends the meat and spawns a **recruit pawn**.
- The recruit walks to that production building (same settlement —
  short, readable trips on existing pawn pathing). On arrival the
  typed cost is spent and the pawn **converts**: it despawns and a
  soldier of the building's kind spawns at the door. If the pool ran
  dry en route, the recruit waits at the building until it can pay —
  a visible queue outside the barracks.
- Recruits are invulnerable and flee combat like workers (arrival
  delayed, never denied). Scattering a village still stalls it.
- More houses = faster army: a capital (8-12 houses, full building
  mix) out-produces a hamlet naturally. The old `train_interval`
  building timer, `tick_training`'s abstract spawner, and
  `train_speed_mult` plumbing are deleted; PRODUCTION row scales
  `recruit_interval` instead.
- **Militia**: neutral houses run the same rule against the shared
  Villager pools; converted militia takes the home-bound DefendZone
  stance, tier-capped per settlement, exactly as today.
- **The player**: free-mode enlistment becomes a call into the same
  conversion function (the player pays no cost — you bring yourself).
  One code path spawns every soldier in the game.

## 4. What this buys (benefits & simplifications)

- **One production rule** for armies, militia, and the player origin —
  the training timer, its per-building cooldowns, and the militia
  special case all collapse into "a pawn arrived at a building".
- **No spawn-from-nothing**: reinforcement pressure is visible as
  people — houses emptying meat into columns of recruits converging on
  barracks. Scouting a rival's villages tells you what's coming.
- **Strategy written in terrain**: pastures feed growth, mines buy
  melee and healers, lumber buys ranged and cavalry. Denying a
  resource type is a real plan (starve their wood → no archers).
- **Three readable numbers** on the HUD instead of abstract stock
  hidden per zone.
- **Symmetry with the villager origin**: the free-mode narrative
  ("everyone was a villager once") becomes literally true of every
  unit on the field.

## 5. HUD

- Under the settlement strip: the war-score row stays; add the player
  faction's three resource counters (MEAT/GOLD/WOOD with pawn-carry
  icons from the pack, or tinted text v1).
- Free-mode pawn phase: resources hidden until enlistment (you own
  nothing); the enlist hint stays.

## 6. Touch points

- `game/mod.rs`: `resources: [[u32; 3]; 5]`, delivery routing by zone
  theme + owner; delete `village_stock`.
- `pawn.rs`: recruit role (target building, arrival callback); reuse
  walk/flee states and sprites (empty-handed run).
- `game/setup.rs`: house spawn timers (replace building
  train_cooldown), conversion function shared with player enlistment
  (`player.rs::tick_conversion` calls it), militia via Villager pool.
- `zone.rs`/`mapgen`: settlement theme exposed per zone (exists via
  `SettlementSpec.theme`); store theme on `CaptureZone` for delivery
  routing.
- `ui.rs`: PRODUCTION row → recruit_interval multiplier; HUD both
  renderers: resource triplet.
- `config.rs`: `recruit_interval` (default ≈ 10 s per house — a
  4-house village ≈ one recruit / 2.5 s attempt rate, gated by meat
  income), typed cost table if tunable.
- bench: report pools; probes must end across the matrix.

## 7. Implementation order

1. **Typed pools & delivery routing** (zone theme → owner pool),
   HUD triplet, bench output; `village_stock` removed with training
   temporarily reading a shim (meat only) so the game stays green.
2. **Recruit pawns**: house spawner, walk-to-building, conversion with
   typed costs; delete `tick_training` + shim; militia via Villager
   pool; player enlistment through the same conversion fn.
3. **Balance & verify**: probe matrix 1v1/1v3 (all end, pacing within
   ~2× of today's), composition-by-territory probe, browser run of
   all three modes, GDD rewrite, screenshots.

## Open questions (deferred, defaults chosen)

- All costs are 1 in v1; the cost table is config if tuning demands.
- Recruit target choice: round-robin across the settlement's
  production buildings (keeps mixed capitals mixed).
- Housing cap: none beyond the army cap in v1 (houses are the rate
  limiter already).
- Worker pawn counts unchanged (one per house, as today).
- Cities keep their single-theme resource ring; mixed income comes
  from the countryside, which is the point.
