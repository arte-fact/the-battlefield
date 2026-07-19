# Village Capture Zones

Capture points become small procedural villages: houses with peons, one
resource being worked, and one or two producing buildings that feed the
controlling faction's army. Peons flee combat. Taking a village is
taking its economy.

## What already exists (build on, don't reinvent)

| Piece | State today |
|---|---|
| `Pawn` (pawn.rs) | Full A* work loop at bases: Idle → WalkToTree → Chop → CarryHome, stuck detection, per-faction sprites |
| `Sheep` (sheep.rs) | Wander + flee-from-units + return, at base pastures |
| `BaseBuilding` (building.rs) | Kinds (Barracks/Archery/Monastery/Castle/Tower/House), footprints, `produces`, procedural base layout |
| Zone tower | `DefenseTower` at each zone center, recolored by zone state (Black = neutral convention, `TOWER_COLOR_FOLDERS`) |
| Mapgen | BSP layout, 7 zones with 6-tile clearing radius, road network through zones |
| Production | `tick_production` spawns waves at the building whose `produces` matches, costs manpower |
| Assets | Pawn Idle/Run/Interact × Axe/Pickaxe/Knife/Hammer + carry Wood/Gold/Meat, all 5 colors; Gold Stones 1–6, Trees/Stumps, Sheep, Meat/Gold/Wood piles; all building kinds in all 5 colors |

## Village anatomy

Each zone gets one **theme** (seed-picked, all three present per map):

| Theme | Resource on the ground | Peon tool / carry | Production building |
|---|---|---|---|
| Mining camp | Gold Stones cluster (3–5) | Pickaxe / Gold | Barracks (warriors + lancers) |
| Lumber camp | Tree grove (4–6, existing loop) | Axe / Wood | Archery (archers) |
| Pasture | Sheep pen (3–4 sheep) | Knife / Meat | Monastery (monks) |

Per village, placed on a ring inside the 6-tile clearing:

- 2–3 **houses** (variants already exist), each home to one peon
- 1 production building (center zones may roll a second, different one)
- the resource cluster on the side away from the main road
- the existing **DefenseTower stays at the center** — it is the capture
  marker and the recolor pattern villages will reuse

Faction bases get their own de-hardcoding pass (section below) built
on the same machinery. Home zones near bases follow the same village
rules; asymmetry comes from themes, not layout size.

## Generation (mapgen)

1. After BSP layout, for each zone: seed a per-zone RNG
   (`seed ^ zone_id`), pick theme and building count.
2. Place footprints on a ring (radius 3–5 tiles) around the center:
   reject positions overlapping roads, water, cliffs, other footprints,
   or leaving the road entrance blocked. Resource tiles go on the
   opposite arc from the road entry.
3. Emit a `VillageSpec { theme, houses, production, resource_tiles }`
   per zone into `MapLayout`; setup.rs instantiates buildings, pawns
   and sheep from it (same split as today: mapgen describes, setup
   spawns).
4. Reuse `paint_road_around_buildings` for dirt aprons; a short road
   stub connects the village ring to the through-road.
5. **Passability is load-bearing**: zones are flow-field targets and
   the settle/capture logic assumes units can enter the circle. Keep a
   clear majority of the capture circle walkable; run the existing
   headless probes after (all 7 seeds must still end).

## Cap-point placement rework

Today the 7 zones sit at fixed diamond percentages (28%/50%/72% along
the base axis, ±16/±30 perpendicular) and `clear_circle(r=6)`
bulldozes whatever terrain is there — a lake or cliff field gets
punched into a circle of grass. Villages need more ground and deserve
better placement:

- **Template + local search**: keep the diamond as the candidate
  template (it encodes the front-line pacing and the fixed adjacency
  graph), but nudge each center within a ±6-tile search window to the
  position that minimizes terrain damage — fewest water/cliff tiles
  inside the village clearing, road-reachable, respecting minimum
  distances. Bulldozing remains the fallback when no candidate is
  clean.
- **Mirrored fairness**: apply nudges symmetrically — B1/R1 and B2/R2
  use mirrored offsets about the map midpoint (same for C1/C3), so
  neither side's villages end up systematically closer or better
  placed. C2 nudges along the axis only.
- **Spacing constraints**, derived not hardcoded:
  `village_clear_radius = ring_max + max_footprint + 1` (≈ 8);
  min zone-to-zone distance ≥ 2 × clear radius + 4; min zone-to-base
  distance ≥ clear radius + base clearing half-extent + 4. Asserted in
  a mapgen test across the bench seeds.
- Clearing grows from r=6 to `village_clear_radius`; the capture
  circle stays `zone_radius = 6` — fights happen inside the village,
  the outskirts are scenery.

## Map size adaptation

128² playable was sized for empty capture circles. Seven villages at
r≈8 clearings plus two 24×28 base rects need more ground or the map
becomes wall-to-wall settlement (~30% of the playable area cleared).

- `Grid` is already dynamic (`width`/`height` fields, all indexing
  through them) — the work is threading actual grid dims through the
  ~8 files that read the `GRID_SIZE` const (fov, ai scoring, combat
  bounds, sheep/pawn clamps, minimap + camera in the renderers), and
  making `generate_battlefield(seed, size)` take the size.
- **v1 sizing**: playable 160 (grid 192) as the single new default —
  villages fit with the same front-line feel; zone template
  percentages are already relative so they scale for free.
- **Perf gate**: tiles go 25.6k → 36.9k (×1.44). Flow-field Dijkstra
  and FOV scale with tile count; the perf audit margin (0.30 ms worst
  frame vs 16 ms budget) absorbs this, but re-run the frame profiler
  and the A*-budget starvation probe at the new size before locking
  it in.
- **Skirmish knob later**: once size is a runtime parameter, a
  MAP SIZE row (Small 128 / Medium 160 / Large 192) in the skirmish
  page is nearly free — deferred to keep v1 focused; noted in the
  config section.

## Base generation (de-hardcoding)

`generate_base_buildings` is halfway there: it has a placer with
footprint/exclusion rules, but every non-house position is a fixed
offset (castle +4, barracks ±6, four set tower spots), and the base
"faces" ±Y via a front-sign even though BSP bases sit on a diagonal —
today's bases literally face the map edge, not the enemy. Rework it on
the same describe-then-spawn machinery as villages:

- **Facing vector, not sign**: front = normalized base→enemy-base
  direction. Castle and gate towers along +front, production in the
  perpendicular flanks, houses and pasture in the −front half-plane.
  Fixes the existing wart and works for any BSP outcome. The
  `fs`-based helpers (`spawn_base_sheep`, gather-point derivation)
  switch to the same vector.
- **Functional bands instead of offsets**: three concentric arcs —
  defense (castle + 3–5 towers), production (one building per unit
  kind in the wave composition, so config changes reshape the base),
  living (8–12 houses + pasture) — each filled by arc sampling with
  the village ring placer's rejection rules. Counts and arc positions
  rolled per seed; the layout stops being recognizable-identical
  every run.
- **`BaseSpec` in `MapLayout`** next to `VillageSpec`: mapgen
  describes both, setup spawns both through one code path. Gather
  point and per-kind spawn buildings are read from the spec instead
  of computed from base center ± constants.
- **Terrain-aware footing**: score the base leaf's candidate centers
  with the same terrain-damage search as villages before the 24×28
  `clear_rect` bulldoze; clear only the bands actually used.
- Same guardrails: determinism test (fixed seed → identical base),
  probes still end, castle/tower defense coverage asserted so a bad
  roll can't leave the front door undefended.

## Peons

Generalize `Pawn` (state names, not three new structs):

- `PawnJob { Chop, Mine, Herd }` — picks target type (claimed tree /
  gold stone / sheep-pen spot), interact + carry sprite rows, and work
  duration. The existing chop loop is the template; mining is
  identical with different sprites; herding walks to a sheep, works,
  carries meat home.
- Resources are infinite in v1 (no stump depletion, no sheep loss);
  the loop is ambience + the delivery hook below.
- **Fleeing**: new `PawnState::Fleeing` reusing the sheep logic —
  trigger when any fighting unit (attack animation or enemy-of-owner
  unit) is within ~4 tiles; drop the carry sprite, run away from the
  threat toward home, resume Idle when clear for a few seconds. Peons
  are not targetable and cannot die in v1.
- Village peons belong to the **zone**, not a faction: neutral zones
  show Black pawns; on capture they swap to the controller's color
  (same dynamic recolor as the tower). Requires embedding Black pawn +
  building sprites in the manifest (wasm bundle size: 8 more building
  PNGs + 1 pawn folder, small).

## Ownership → army production

Village buildings recolor with zone state (extend the tower's dynamic
faction to all village buildings). While a faction controls a village:

- The village's production building **adds its unit kinds to that
  faction's wave** (`build_wave` reads controlled villages) and those
  units **spawn at the village building** — the existing
  spawn-at-producer lookup already does this once building faction
  flips. Reinforcements appearing at the front line is the strategic
  payoff.
- Units from villages still **cost manpower** — villages raise
  production rate and add spawn points; the manpower pool stays the
  single strategic resource (bleed/annihilation untouched).
- **Delivery stock**: each peon delivery adds 1 stock to the village
  (cap ~5). Producing a unit at the village consumes 1 stock; empty
  stock pauses that village's extra production. This is what makes
  peon activity and fleeing matter: marching an army through a village
  scatters the peons and stalls its output without needing to hold it.

## Config knobs (GameConfig)

`village_houses (2–3)`, `village_stock_cap`, `village_delivery_time`
(work duration), `village_wave_bonus` (units per wave per village),
`pawn_flee_radius`, `playable_size` (default 160). Skirmish rows
("VILLAGES: ON/OFF", "MAP SIZE: S/M/L") become possible later — not
in v1.

## Roadmap (checkable)

- [x] 1. Map size: thread runtime grid dims through the `GRID_SIZE`
  readers (core + both renderers' minimap/camera), size parameter on
  `generate_battlefield`, default playable 160; frame profiler +
  starvation probe re-run at the new size.
- [x] 2. Placement: local-search nudging with mirrored fairness,
  derived spacing constraints + mapgen test, terrain-damage scoring,
  clearing at `village_clear_radius`.
- [x] 3. Mapgen: `VillageSpec` + ring placement with rejection rules;
  themes distributed per map; roads stubs; probes still end on all 7
  bench seeds.
- [x] 4. Base rework: facing vector, functional bands via the ring
  placer, `BaseSpec` unifying the describe-then-spawn path with
  villages, terrain-aware footing; determinism + defense-coverage
  tests.
- [x] 5. Buildings: village buildings instantiated from spec, dynamic
  faction recolor (generalize the tower path), Black neutral assets in
  the manifest + both renderers.
- [x] 6. Resources on the ground: gold stones / groves / pens as
  decorations with passability flags; pasture sheep reuse `Sheep`.
- [x] 7. Pawn jobs: `PawnJob` refactor of the chop loop, mine + herd
  variants, carry sprites, village pawns spawned from spec, neutral
  color + recolor on capture.
- [x] 8. Fleeing: combat detection, flee/return states, drop-carry;
  peons untargetable.
- [x] 9. Production: delivery stock, wave bonus from controlled
  villages, spawn at village building; tests (stock consumption, wave
  composition, stall when scattered).
- [x] 10. Verify: fixed-seed sim dumps byte-identical; P99 frame
  0.61ms at the 192 grid (16.7ms budget); all 7 bench seeds end
  (336-624s, both factions win); 218 core tests + workspace clippy
  green; headless screenshots confirm the rotated bases, neutral
  assets and battle flow (blind keyboard walking never framed a
  village close-up -- code path shared with verified building
  rendering). GDD updated.

Deviation from plan: no separate `BaseSpec` -- bases kept their
describe-and-place generator in building.rs, gaining the facing
vector, seeded bands and derived clearing; a spec struct added no
value since bases spawn nothing zone-owned. Village stock starts at
2 so early captures pay off immediately.

## Decisions taken (flag if you disagree)

- Peons are invulnerable and neutral-to-the-zone (recolor on capture);
  killable peons / raid scoring is a later layer.
- Village units cost manpower — villages multiply *where and how fast*
  the pool converts to soldiers, never create free soldiers.
- DefenseTower remains the capture marker at village center.
- Resources are infinite; stock (not resource depletion) is the
  economic lever.
- Base rework changes layout generation only — castle/tower stats,
  wave composition and manpower are untouched.

## Out of scope

- Player commanding peons; building construction/repair (Hammer
  sprites stay unused).
- Resource counters in the HUD; trading; per-resource differences
  beyond flavor + which building they feed.
- Goblin/other-faction villages (only 5 human colors in the free
  pack anyway).
