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
- **Capitals become mixed economies**: the City resource ring changes
  from one theme to **thirds of all three** (pasture arc, gold arc,
  grove arc), and its house workers split jobs accordingly. Without
  this, a faction whose capital rolled a gold theme and who owns no
  pasture would produce **zero meat income and therefore zero
  recruits** once its starting pool runs out — a bootstrap
  death-spiral decided by the map roll. Mixed capitals guarantee every
  faction a baseline of all three incomes; the countryside then tilts
  the balance. (1v1 mirror fairness is automatic — both capitals get
  the same thirds.)

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
  fire, if the faction has 1 meat, has cap headroom, and the
  settlement has a production building whose typed cost is currently
  affordable, the house spends the meat and spawns a **recruit pawn**.
- **Cap headroom counts in-flight recruits**: a house spawns only if
  `alive units + walking recruits < max_units_per_faction`, or a
  full army with five recruits en route overshoots the cap on
  arrival.
- The recruit walks to that production building (same settlement —
  short, readable trips on existing pawn pathing). On arrival the
  typed cost is spent and the pawn **converts**: it despawns and a
  soldier of the building's kind spawns at the door.
- **Queue, don't deadlock**: if the pool ran dry en route the recruit
  waits at the building — a visible queue outside the barracks — but
  after ~15 s it retargets to any affordable production building in
  the settlement, and idles near its house if none is. Without the
  retarget, a faction that lost all wood income would strand every
  archery-bound recruit forever while gold piled up unused. A
  settlement also stops spawning recruits beyond 2× its production
  buildings in flight (no ghost-town crowds).
- **Zone flips abort recruits**: recruits are faction-bound (they
  spent their faction's meat — they render in faction colors via the
  existing `zone_id: None` pawn path, unlike zone-bound workers who
  recolor with their village). If the recruit's settlement is no
  longer Controlled by its faction on arrival or retarget, it fades
  out. Landless factions thus wind down cleanly: no settlements → no
  houses → no recruits, and the starvation rule finishes the remnant
  army as before.
- Recruits are invulnerable and flee combat like workers (arrival
  delayed, never denied). **Emergent siege**: a battle raging at a
  settlement keeps its recruits fleeing instead of converting — you
  choke a capital's production by fighting at its gates, no blockade
  mechanic needed.
- More houses = faster army: a capital (8-12 houses, full building
  mix) out-produces a hamlet naturally. The old `train_interval`
  building timer, `tick_training`'s abstract spawner, and
  `train_speed_mult` plumbing are deleted; PRODUCTION row scales
  `recruit_interval` instead. House timers reuse the existing
  `train_cooldown` field, staggered per house.
- **Militia**: neutral houses run the same rule against the shared
  Villager pools; converted militia takes the home-bound DefendZone
  stance, tier-capped per settlement, exactly as today. Militia sleep
  is untouched (recruits are pawns — they never enter unit AI).
- **The player**: free-mode enlistment becomes a call into the same
  conversion function (the player pays no cost — you bring yourself).
  One code path spawns every soldier in the game. Standing near a
  friendly barracks as fresh soldiers pop lets your authority
  vacuum-recruit them straight into the retinue — the drill-sergeant
  loop comes free.

## 4. Interactions with existing mechanics (audited)

| System | Interaction | Resolution |
|--------|-------------|------------|
| Conquest victory | Standing = settlements ∪ units; recruits are pawns, not units | No change — recruit-only factions can't exist (recruits fade with their settlements) |
| Starvation | Landless faction: no houses → no new recruits | Consistent; starvation still finishes remnant armies |
| Army cap | In-flight recruits could overshoot | Headroom counts walking recruits (above) |
| Composition | Round-robin over buildings ⇒ capitals ~20% monks (was ~10% with wave shares) | Watch item; tuning lever = building pick weights, only if probes/play show monk bloat |
| War score / planner | Unchanged; typed income is invisible to the AI in v1 | Future lever: planner weight for the resource type the faction lacks — noted, not built |
| Militia sleep | Recruits are pawns, outside unit AI | No interaction |
| Retinue / authority | Fresh converts are ordinary recruitable soldiers | Feature (drill-sergeant loop) |
| Free-mode enlistment | Same conversion fn, cost-free for the player | Unifies the code path |
| START AS colors | Conversion checks zone Controlled(faction) — works for any army | No change |
| Worker peons | Deliveries reroute to typed pools by zone theme (stored on CaptureZone) | Workers, fleeing, sheep herding all unchanged |
| Capture flow | Contested zones already pause work; flips abort faction recruits | Rule above; no loot (pools are faction-wide) |
| Determinism | House timers staggered by index, fixed iteration order | Same discipline as train_cooldown today |
| Performance | Recruit count bounded by cap headroom + per-settlement 2× rule | Pawn updates are cheap; no AI cost |
| wasm config hooks | `recruit_interval` replaces `train_interval` in the JSON | serde defaults keep old payloads harmless |

## 5. What this buys (benefits & simplifications)

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

## 6. UI update

Art comes free with the pack: `Terrain/Resources/*/{Gold_Resource,
Meat Resource, Wood Resource}.png` are clean drop icons, and the unit
avatar portraits (Warrior/Lancer/Archer/Monk) are already loaded.
Everything below is zero-input UI — no new buttons, both renderers,
touch-safe.

### 6.1 Resource tray (top-left, under the authority bar)

- Three icon+number pairs — MEAT, GOLD, WOOD — for the **player's
  faction only** (player-private info lives left with HP/authority;
  faction-comparative info stays center). Icons at ~24 px from the
  resource drop sprites, numbers in HUD white.
- The number does a brief **scale pulse when it changes** so income
  and spending are felt without reading.
- Hidden during the free-mode pawn phase (you own nothing yet) and
  replaced by nothing — the hint (6.4) carries that phase.

### 6.2 Delivery & spend ticks (floating world text)

- A peon banking a delivery pops a small floating `+1` **tinted by
  resource** (meat red, gold yellow, wood green) above the drop-off,
  reusing the FloatingText machinery (gains a `kind` so authority
  popups keep their look).
- Conversions pop `-1` in the same tint above the building. Income
  and spending are visible **in the world**, at the place they
  happen — the tray is just the sum.

### 6.3 Production plates (world UI, on production buildings)

- A small paper chip above a production building, in the style of the
  zone name ribbons, shown when it has something to say:
  - **Queue stalled**: recruits waiting outside and the typed cost
    unaffordable → the missing resource icon, blinking gently. A
    glance at the map explains every stall ("archery is out of wood").
  - **Player nearby & unaligned** (free mode / skirmish NEUTRAL): the
    building's unit **avatar portrait** + its cost icon — you see
    what you would become before stepping in. Neutral buildings show
    the ribbon with "SERVES NO LORD".
  - Otherwise: no plate (the world stays clean).

### 6.4 Free-mode hint

- The static "ENTER A PRODUCTION BUILDING TO ENLIST" banner stays
  until the player first comes within plate range of any production
  building, then retires for the run — the plates (6.3) take over as
  the actual affordance.

### 6.5 Settlement name plates

- The zone ribbon gains its settlement's **theme icon** to the left of
  the name: steak for pastures, nugget for mining camps, log for
  lumber camps. Capitals (mixed thirds) show the trio at half size.
  One glance at a banner tells you what taking this place buys.

### 6.6 Unchanged / deferred

- War-score row, settlement pip strip, minimap, menus, score screens:
  unchanged. PRODUCTION skirmish row already covers pacing.
- Deferred (noted, not v1): theme rings on minimap settlement dots; a
  distinguishing mark on recruit pawns vs workers (motion — walking
  to a door, queueing — reads well enough first); pool-cap warning
  when a resource sits at 99 with nothing to spend it on.

### 6.7 Plumbing

- Embed the three resource PNGs; `SpriteKey::ResourceIcon(0..3)` in
  the shared renderer + lookups in both asset loaders (reserve any
  new wgpu texture slots **before** the text-cache base id — the
  fog-slot lesson).
- Plate quad/anchor math shared in `render_util` (like the zone
  ribbons); drawing per-renderer as today.
- FloatingText gains a `kind: {Authority, Resource(u8)}` and a tint
  table.

## 7. Touch points

- `game/mod.rs`: `resources: [[u32; 3]; 5]`, delivery routing by zone
  theme + owner; delete `village_stock`.
- `pawn.rs`: recruit role (target building, arrival callback); reuse
  walk/flee states and sprites (empty-handed run).
- `game/setup.rs`: house spawn timers (replace building
  train_cooldown), conversion function shared with player enlistment
  (`player.rs::tick_conversion` calls it), militia via Villager pool.
- `zone.rs`/`mapgen`: settlement theme exposed per zone (exists via
  `SettlementSpec.theme`); store theme on `CaptureZone` for delivery
  routing. `mapgen/village.rs`: City resource ring becomes mixed
  thirds (map-hash probe will change for capitals — expected, note in
  the commit); city house workers split jobs by arc.
- `ui.rs`: PRODUCTION row → recruit_interval multiplier; HUD both
  renderers: resource triplet.
- `config.rs`: `recruit_interval` (default ≈ 10 s per house — a
  4-house village ≈ one recruit / 2.5 s attempt rate, gated by meat
  income), typed cost table if tunable.
- bench: report pools; probes must end across the matrix.

## 8. Implementation order

1. **Mixed capital rings** (mapgen): City ring in thirds, workers
   split by arc — lands first because every faction's baseline income
   depends on it; livability tests updated, map hashes re-baselined.
2. **Typed pools & delivery routing** (zone theme → owner pool),
   resource tray + delivery ticks (UI 6.1-6.2), settlement plate
   theme icons (6.5), bench output; `village_stock` removed with
   training temporarily reading a shim (meat only) so the game stays
   green.
3. **Recruit pawns**: house spawner with cap-headroom counting,
   walk-to-building, conversion with typed costs, the retarget and
   zone-flip rules; production plates + hint retirement (UI 6.3-6.4);
   delete `tick_training` + shim; militia via Villager pool; player
   enlistment through the same conversion fn.
4. **Balance & verify**: probe matrix 1v1/1v3 (all end, pacing within
   ~2× of today's), composition-by-territory probe (armies reflect
   owned themes; monk share sane), browser run of all three modes,
   GDD rewrite, screenshots.

## Open questions (deferred, defaults chosen)

- All costs are 1 in v1; the cost table is config if tuning demands.
- Recruit target choice: round-robin across the settlement's
  production buildings (keeps mixed capitals mixed); monk share is
  the watch item.
- Housing cap: none beyond army-cap headroom counting (houses are the
  rate limiter already).
- Worker pawn counts unchanged (one per house, as today).
- Recruit walk speed: worker pace v1; a hair faster if reinforcement
  latency feels sluggish in play.
- Starting armies unchanged (free, instant) — the pipeline is for
  reinforcements, so early-game pacing is untouched.
