# City Network & Multi-Faction Battles

No more "main base vs capture points": the map is one road network of
settlements of varying sizes, **all capturable**. The largest cities
are the starting holdings of the player colors. 1v1 up to 1v3 (single
player + AI for now), using the pack's four army colors — Blue, Red,
Yellow, Purple — with Black staying the villagers.

## Settlement tiers

One generator, one data model (`SettlementSpec` generalizes
`VillageSpec`), tier picks the numbers:

| Tier | Houses | Production | Defense | Resource | Garrison cap | Capture radius |
|---|---|---|---|---|---|---|
| Hamlet | 2 | 1 | tower | 1 cluster | 2 | 5 |
| Village (today's) | 3-4 | 1-2 | tower | 1 cluster | 4 | 6 |
| Town | 5-6 | 2-3 | 2 towers | 2 clusters | 6 | 7 |
| City (capital) | 8-10 | full set (all 4 kinds) | castle + 3-4 towers | pasture + cluster | 8 | 8 |

- Cities replace bases: castle band + production + housing, generated
  by the same facing-free ring/band placer, **and they are capture
  zones** — everything on the map can change hands.
- Reinforcement waves spawn at your **largest controlled settlement**
  with the needed production building (fallback: any controlled one);
  rally happens there. Lose it, production shifts to the next one.
  Losing your last settlement doesn't kill you — annihilation stays
  pool+army based.
- Garrisons, peons, stock, militia: exactly the shipped village
  system, scaled by tier.

## Very large maps — what actually breaks (analysis)

The goal is "very, very large". Audit of what scales with tile count
in the current architecture (per-tile costs at playable 224² = 50k,
512² = 262k, 1024² = 1M tiles):

| System | Scaling | 512² | 1024² | Verdict |
|---|---|---|---|---|
| Grid memory (~12 B/tile across vecs) | linear | ~3 MB | ~13 MB | fine |
| Mapgen (simplex + automata, one-time) | linear | ~1 s | ~4 s | fine (loading moment) |
| **Flow fields: full-grid Dijkstra × zone** | tiles × zones | 30 zones ≈ **47 MB, slow setup** | ≈ **190 MB** | **breaks** |
| FOV compute (radius-limited) | constant | — | — | fine |
| Fog texture upload (on fog_dirty) | linear | 262 KB | 1 MB | fine, already gated |
| Renderer world pass (view-culled) | constant | — | — | fine |
| Units / combat / separation (capped + spatial hash) | constant | — | — | fine |
| Pawns/sheep (per settlement, short-range A*) | settlements | — | — | fine |

One system breaks: full-grid per-zone flow fields. Streaming/chunked
terrain is NOT needed at these sizes — memory and rendering are
already fine to 1024²; the fix is **hierarchical navigation**:

1. **Roads are the strategic layer.** Mapgen already produces the
   settlement graph with road polylines. A unit far from its target
   settlement follows a road path (graph A* over settlements — a few
   dozen nodes — then steering along the polyline). No grid Dijkstra
   involved, map-size independent, and armies marching in columns
   down roads is exactly the fantasy.
2. **Local flow fields, bounded extent.** Per-settlement fields
   compute only within an influence radius (~32 tiles): memory and
   time per field become constant (≈25 KB) regardless of map size.
   Units switch road-following → local flow when they enter the
   target's influence.
3. **Off-road behavior unchanged.** Chases, player wandering, flee:
   the bounded best-effort A* and steering are already map-size
   independent.

**Committed ceiling: 1024² playable (~150 settlements).** Two
supporting pieces beyond navigation:

- **Budgeted (chunked) generation**: terrain must exist map-wide at
  battle start (the sim marches everywhere from t=0), so chunking
  means the generation pipeline — noise, automata passes, settlement
  placement, roads, decoration — runs as budgeted steps across
  frames behind a loading bar instead of one frozen multi-second
  frame. Wasm main thread never blocks > ~50 ms.
- **Militia sleep**: 150 settlements would field ~400+ garrison/
  militia units. Garrisons far from any enemy (spatial-hash check)
  sleep: no AI tick, no separation, no animation — they wake when a
  hostile enters their settlement's influence radius. Keeps live
  unit cost proportional to active fronts, not map size.
- **Roads are highways**: +25% movement speed on road tiles for all
  units. Combined with road-following navigation, armies visibly
  column-march between settlements and road control matters.

Rollout is phased: AUTO sizes ship first on the new navigation, then
LARGE (384²) and HUGE (512², then 1024²) unlock behind the perf
gates below.

## Network generation

1. **Count & sizes from map size**: AUTO playable 192 (1v1) to 224
   (1v3); the MAP SIZE row unlocks LARGE 384 and HUGE 512+ once
   hierarchical navigation passes its gates. Settlement count scales
   ~linearly with area: one city per player color, towns ~1 per 3
   villages, from ~10 settlements (192²) to ~40 (512²).
2. **Placement**: capitals first, maximally separated (max-min
   distance sampling on the playable disc, mirrored-fair for 1v1);
   then towns and villages by best-candidate sampling (highest
   distance to existing settlements + terrain-damage scoring from the
   village placer). Derived spacing constants, as today.
3. **Roads = the graph**: Kruskal MST over settlements (exists) plus
   the k shortest non-MST edges (~20% extra) for loops. The road
   edges ARE the zone adjacency graph the AI plans over — the
   hardcoded 7-zone diamond dies.
4. Peons, resources, aprons: unchanged machinery per settlement.

## Multi-faction core (the deep change)

- `Faction::{Blue, Red, Yellow, Purple, Villager}` with
  `fn idx() -> usize` (0..4); every `[T; 2]` keyed by faction becomes
  `[T; 4]` (manpower, spawn queues, macro objectives, planner
  targets). `active_factions: Vec<Faction>` from config drives all
  loops — code stops enumerating Blue/Red literally.
- **Capture model rework**: the signed Blue/Red scalar cannot express
  four factions. `CaptureZone` gets
  `owner: Option<Faction>`, `capturing: Option<Faction>`,
  `progress: f32 (0..1)`, `counts: [u16; 4]`. Majority rule
  preserved: the strongest present faction captures at
  √(its count − all others inside), ties freeze. Membership
  hysteresis, state debounce via sticky counts, and rendering states
  map 1:1 onto the new fields.
- **Flow fields deduped**: per-zone fields are terrain-only (verified:
  identical per faction, cached) — one shared set replaces
  blue_flow/red_flow, so more factions cost nothing here. Objective
  scoring/planning stays per faction (cheap, staggered).
- **Victory (FFA)**: everyone fights everyone.
  - Domination: hold **more than half of all settlements** for the
    hold time.
  - Annihilation: pool empty + army dead eliminates a faction
    (its garrisons convert to... nothing — they fight on as-is);
    last faction standing wins.
  - Bleed: a faction holding ≥ half the settlements bleeds all
    rivals; sudden death (all pools empty) = most settlements held.
- **AI targeting — one weighted score**: `score_all_zones` gains
  simultaneous, individually tunable terms on top of the current
  base: `ai_w_leader` (zones held by the current settlement leader
  score higher — the pack naturally turns on whoever is winning,
  including the player) and `ai_w_neighbor` (zones adjacent to own
  territory via the road graph score higher — regional wars). All
  active at once with config weights; pure FFA is just both weights
  at 0. No hard alliances.

## Renderers & HUD

- Yellow/Purple unit + building sheets: loaders already iterate
  faction lists; add the two colors and embed (~30 sheets, same as
  the Black batch).
- HUD: manpower readouts and zone pips become per-active-faction
  (colors from a single `faction_rgb()` helper — kills today's
  scattered Blue/Red match arms). Minimap dots likewise.
- Map size becomes runtime: fog/minimap textures in both renderers
  allocate from `game.grid` dims at setup (and recreate when a new
  battle changes size) instead of the `GRID_SIZE` const — required
  now that 1v1 and 1v3 maps differ.

## Config & modes

- `GameConfig`: `enemy_count: 1-3` (drives active factions
  Blue+Red[+Yellow][+Purple]), `playable_size` auto = 192 + 16 ×
  (enemy_count-1) unless overridden.
- Skirmish page: new ENEMIES row (1/2/3); MAP SIZE row (AUTO/S/M/L)
  becomes real now that size is runtime.
- Arcade: **escalating ladder** — a run starts 1v1; every victory
  adds an enemy color to the next run (1v1 → 1v2 → 1v3, capped),
  defeat resets the ladder to 1v1. Run score is multiplied by the
  enemy count. The ladder level persists with the scoreboard storage
  and shows on the menu ("ARCADE — LEVEL 2 (1v2)").
- Player is always Blue (single player); playable-color row stays
  deferred.

## Performance gates

- Units: 4 × 35 cap + garrisons ≈ 190 worst case (~2× today).
  Separation/combat use the spatial hash (linear-ish); perf audit
  margin is ~20×. Gate A: P99 ≤ 2 ms at 224², 1v3, full armies.
- Gate B (unlocks LARGE/HUGE 512²): P99 ≤ 3 ms and setup ≤ 5 s at
  512², 1v3, ~40 settlements, on hierarchical navigation with
  bounded local fields; battles still end on all bench seeds.
- Gate C (unlocks 1024²): P99 ≤ 4 ms with militia sleep active,
  budgeted generation ≤ 8 s with no frame > 50 ms, memory < 300 MB
  wasm.
- Flow: shared + bounded fields make cost per settlement constant;
  road-graph A* is over dozens of nodes.
- In 1v3, per-faction unit cap scales down (e.g. 28) to keep totals
  and pacing sane; skirmish row still overrides.

## Roadmap (checkable)

- [x] 1. Faction plumbing: 4 army colors + idx(), active_factions,
  arrays to [4], one faction_rgb()/color-helper pass over renderers;
  Yellow/Purple sheets embedded and loaded. Compiles and plays with
  2 active factions, zero behavior change.
- [x] 2. Capture rework: owner/capturing/progress/counts model,
  majority + membership hysteresis + sticky-target logic ported;
  zone tests rewritten; 1v1 probes still end on all seeds.
- [x] 3. Multi-faction battle flow: victory (domination >half +
  last-standing), FFA bleed, sudden death, elimination; weighted AI
  targeting (ai_w_leader + ai_w_neighbor on the base score);
  per-faction production; 1v2/1v3 probes end.
- [x] 4. Settlement tiers: SettlementSpec, tiered generation via the
  ring/band placer, city tier absorbs base generation, garrisons by
  tier; spawn/rally at largest controlled settlement.
- [ ] 5. Network placement: capital max-separation, best-candidate
  towns/villages, MST + ~20% extra road edges, adjacency from road
  edges; invariants generalized (spacing, fairness, livability,
  reachability).
- [ ] 6. Roads as highways: +25% speed on Road tiles; road-following
  steering polish so columns look right.
- [ ] 7. Hierarchical navigation: shared (deduped) then bounded local
  flow fields (~32-tile influence), settlement-graph A* +
  polyline following, handoff at influence edge; equivalence checks
  on AUTO sizes (battles end, durations comparable).
- [ ] 8. Runtime map size in renderers: fog/minimap textures from
  grid dims, recreated on battle setup.
- [ ] 9. Budgeted generation pipeline + loading bar (no frame >
  50 ms); militia sleep (wake on hostile in influence radius).
- [ ] 10. Config & UI: ENEMIES row (1/2/3), MAP SIZE row
  (AUTO/LARGE/HUGE, gated), per-faction HUD (manpower, pips,
  minimap); arcade escalation ladder (level persisted with scores,
  reset-on-defeat, score × enemy count, level on menu).
- [ ] 11. Perf gates: A (224² 1v3 ≤ 2 ms), B (512² ≤ 3 ms, unlocks
  LARGE/HUGE), C (1024² ≤ 4 ms + generation budget, unlocks the
  ceiling).
- [ ] 12. Verify: determinism per size/enemy-count, all-seed probes
  1v1 and 1v3 (all end, no faction stalls), headless screenshots
  (4-color battle, city, town, road column), GDD rewrite, roadmap.

## Decisions taken (flag if you disagree)

- FFA: AIs fight each other as readily as the player; no teams v1.
- Arcade escalates 1v1→1v3 across victories, resets to 1v1 on
  defeat; score scales with enemy count.
- Large-map tech: hierarchical navigation + budgeted generation;
  ceiling 1024² playable; roads +25% speed.
- Eliminated factions' surviving units fight on (no despawn); their
  settlements are capturable loot.
- Villager militia (Black) unchanged — neutral settlements of every
  tier raise it.
- Player color fixed Blue.

## Out of scope

- Teams/alliances, diplomacy, multiplayer netcode.
- Playable non-Blue colors (row still deferred).
- Per-city upgrades, walls/siege; naval anything.
