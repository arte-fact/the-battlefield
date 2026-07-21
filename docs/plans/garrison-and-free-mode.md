# Garrison Improvements & Free Mode — Spec

Three features: garrisons that feed the retinue, garrison monks that
earn their keep, and a new default game mode where every run begins as
a neutral villager who picks a side by walking into a war.

Decisions taken with the designer (2026-07-21):

- Garrison soldiers are recruitable **anytime**; the village refills.
- Conversion happens at **army-owned production buildings only**.
- The neutral pawn is **killable by stray hits** (never targeted).
- The villager origin powers **FREE mode** — the new default mode
  beside Arcade and Skirmish: the full-featured game with **no timed
  objectives**. Arcade keeps the classic Blue-soldier start and the
  ladder.
- Skirmish gains a **START AS** row: begin as any army color (soldier
  at that faction's capital) or NEUTRAL (villager pawn origin).

---

## 1. Garrison joins the retinue

**Today:** units under `DefendZone` are excluded from recruitment
(`is_retinue_order`), from follower counts, and from the planner. The
only way to get them back is the long-press Defend re-recruit visit.

**Spec:**

- Garrison soldiers of the player's faction become **normal recruitment
  candidates**: when the player is inside their zone's influence and the
  per-second authority roll passes, they leave the garrison and join the
  retinue like any field soldier. No new input — walking by is enough,
  which matches the existing "your wake vacuums up idle allies" rule.
- Recruitment clears their `DefendZone` order (they become sticky
  followers). The zone's garrison headcount drops accordingly.
- **The village refills**: garrison spawning already runs on village
  stock (`tick_village_garrisons`, one soldier per `~6 s` while stock
  lasts and headcount < tier cap). No new mechanism needed — recruiting
  defenders simply re-opens headroom, and the village spends stock to
  replace them. Draining a garrison is therefore a real trade-off: the
  zone is exposed until deliveries rebuild stock.
- **Neutral (Black) militia is never recruitable** — only garrisons of
  the player's own color (captured villages producing in your color, or
  soldiers you stationed via long-press Defend).
- Authority still gates everything: recruitment chance, command radius,
  and the follower cap are unchanged. A full retinue recruits nothing.
- Long-press Defend (stationing) keeps working as the inverse flow and
  keeps its re-recruit cooldown, which now simply means "this soldier
  refuses to re-join for 12 s", consistent with dismissal.

**Touch points:** `orders.rs` (recruitment filter), `setup.rs`
(garrison tests), HUD unchanged.

**Tests:** player with high authority walks into an owned village →
garrison soldiers join up to the cap; village stock then refills the
garrison; Black militia never joins; re-recruit cooldown respected.

## 2. Garrison monks that do something

**Today:** a Monk in `DefendZone` stance only acts when an enemy is
inside the leash — and its kind-AI is "flee and heal", which at a quiet
post means standing at the hold point doing nothing. Wounded garrison
mates and passing armies heal at zero benefit from having a monk
stationed.

**Spec:**

- While its zone is quiet, a garrison monk **patrols between wounded
  friendly units inside the zone radius** (garrison mates, retinue
  members, passing field soldiers of the owner faction — anyone
  friendly) and channels its normal heal on the most wounded target in
  range. With nobody wounded, it walks back to its hold point.
- Militia sleep still applies: the whole check is skipped while the
  zone sleeps, EXCEPT that a zone with a wounded friendly unit inside
  counts as awake for its monks only (cheap: reuse the influence query
  already computed in `tick_ai`; a wounded-ally scan only runs when the
  zone has a stationed monk).
- When enemies are inside the leash, current behavior stands (kind-AI:
  monks stay back and heal fighters).
- Village production choice is untouched — pasture villages produce
  monks, so pasture garrisons become the healing stops of the road
  network. That is a feature: wounded armies detour through friendly
  pastures.

**Touch points:** `orders.rs::ai_order_defend_zone_tick` (monk arm),
`ai.rs` (sleep gate exception), no config changes beyond an optional
`zone_monk_heal_radius` if the zone radius proves wrong.

**Tests:** wounded warrior in an owned pasture village regains HP from
the stationed monk while no enemy is near; monk returns to its hold
point when everyone is healthy; sleeping zones with no wounded stay
asleep.

## 3. Free mode — start as a neutral pawn

**The narrative:** you are nobody. A villager with a tool, in a
countryside being torn apart by up to four armies. Walk the roads, see
the war, and choose whom to serve — the moment you step into a
barracks, you stop being a bystander.

### Mode structure

- **FREE** joins `GameMode` as the **first, default button** on the
  main menu, above ARCADE and SKIRMISH.
- Free mode is the **full-featured game with no timed objectives**:
  the domination hold timer, sudden death, and leader-deficit bleed
  are all disabled. The war ends only by real events — annihilation /
  last army standing, or the player's death. Stalemates are allowed;
  it is the sandbox where you live in the world.
- Everything else is on: villages, garrisons, economy, capturable
  capitals, authority, retinue, fog.
- Free mode uses AUTO map sizing with **four armies** (1v3-strength
  map), so the pawn has real factions to choose between. No setup
  screen — one press and you are in the world; Skirmish is where you
  configure.
- All armies are AI from the first tick. Until conversion there is no
  "player faction" — every army is autonomous (the Blue-is-player
  special casing must be behind a `player_faction: Option<Faction>`
  instead of a hardcoded `Faction::Blue`).

### Skirmish: START AS row

Skirmish gains a **START AS** row: `NEUTRAL / BLUE / RED / YELLOW /
PURPLE` (colors above the enemy count are hidden).

- A color: classic soldier start — you spawn as that faction's Warrior
  at its capital, full kit from the first tick. Blue remains the
  default, so existing skirmish behavior is one row-cycle away.
- NEUTRAL: the villager-pawn origin below, inside skirmish's
  configured battle (timed objectives stay on in skirmish — the rows
  already control hold time and bleed).

### The pawn phase

- The player spawns as a **Villager-faction pawn** at a neutral
  countryside village near the map middle (farthest neutral settlement
  from all capitals, so the walk shows the network).
- Controls: movement only. Attack/Charge/Defend/Dismiss do nothing (or
  show a "you are nobody yet" shrug hint). No authority, no retinue,
  no recruitment pings.
- **Killable by stray hits**: soldiers and towers never target the
  pawn, but area damage — melee cone swings, arrow impacts — hurts it
  (normal pawn-sized HP, say 5). Dying as a pawn ends the run with the
  survival-seconds score only. The war is dangerous to bystanders;
  crossing a battle line is a choice.
- Fog of war follows the pawn as usual. The HUD hides authority,
  retinue and manpower rows during the pawn phase and shows a single
  hint: *"Enter a production building to enlist."*

### Conversion

- Trigger: the pawn steps within ~1 tile of a **production building
  owned by an army faction** (Barracks, Archery Range, Monastery — at
  capitals or captured villages). Neutral villages' buildings do not
  convert; near them the hint reads *"This village serves no lord."*
- Effect, instantly and permanently for the run:
  - Player unit becomes the building's produced **unit kind** in the
    building's **owner faction** (Barracks → Warrior, Archery → Archer,
    Monastery → Monk; Lancer via the Barracks' lancer line if present —
    exact mapping: whatever `production kind` the building already has).
  - Full player kit switches on: attack, orders, authority (starting at
    0 plus skirmish-style `start_authority` if configured), retinue,
    fog radius, manpower HUD for the joined faction, capture
    credit, the works.
  - `player_faction` is set; the joined army's planner treats the
    player exactly as Blue is treated today (idle-army handicap
    included). Victory/defeat are evaluated from that faction's side.
  - A small ceremony beat: order-pulse ring + floating text
    ("You serve the Red banner"), so conversion reads on screen.
- There is no un-conversion and no second conversion.

### Scoring & end conditions

- **Free mode is unscored** — Arcade stays the scored ladder. Free
  runs end on the victory/defeat/death screens without the score
  entry flow.
- Run ends: pawn death, converted-player death, or the joined faction
  winning by last-army-standing / being eliminated. With no timed
  objectives, "the war ended without you" can only happen through
  full annihilation among the AIs — allowed, and the run ends on the
  defeat screen with that line.
- Skirmish NEUTRAL starts keep skirmish's usual end conditions and
  result screens (skirmish is already unscored).

### Technical notes

- `player_faction: Option<Faction>` on `Game` replaces the implicit
  Blue; `is_player` stays on the unit. HUD, planner, victory checks,
  bleed, and arcade code read Blue via one accessor today-equivalent
  (`game.player_faction().unwrap_or(Faction::Blue)`), so Arcade and
  Skirmish behave byte-identically.
- The pawn phase reuses the existing pawn sprite/animation set for the
  Villager color; the player pawn is a `Unit` with a pawn skin, not a
  `Pawn` (it needs input, FOV and death handling).
- Conversion swaps `kind`, `faction`, stats and sprite on the existing
  player unit — no respawn, position preserved.
- Menu: FREE as the new default button + `GameScreen` flow unchanged
  (Loading → Playing). Mode stored in `UiState.mode`; free-mode setup
  is `enemy_count = 3`, AUTO size, timed objectives off
  (`victory_hold_time` ignored, sudden death and bleed disabled via
  the mode, not via config edits — skirmish rows keep their meaning).

## Suggested implementation order

1. **Garrison recruitment** (small, isolated): recruitment filter +
   refill tests.
2. **Garrison monk patrol/heal** + sleep exception + tests.
3. **`player_faction` refactor** — mechanical sweep replacing hardcoded
   Blue-as-player assumptions; probes must stay byte-identical.
4. **Skirmish START AS row** (colors first — small once
   `player_faction` exists; NEUTRAL value lands with step 5).
5. **Free mode**: menu entry, untimed rules, pawn spawn/phase,
   stray-hit damage, conversion, HUD states, end conditions; the same
   pawn origin wired to skirmish's NEUTRAL.
6. **Verify**: probes (all modes), browser run of a full free-mode
   loop (spawn → walk → enlist with each of the 3 building kinds →
   fight), GDD section, screenshots.

## Open questions (deferred, defaults chosen)

- Should stationed (long-press) soldiers also be auto-recruitable now
  that garrisons are? Spec says yes — one rule for all garrisons.
- Pawn walking speed: normal pawn speed vs unit speed (v1: unit walk
  speed, so the opening isn't slow).
- Free mode is unscored (Arcade owns the scoreboard); revisit if free
  runs want bragging rights.
- "The player is Blue" leaves the HUD too: minimap/unit visibility
  rules ("hide enemies outside FOV") key on `player_faction`.
