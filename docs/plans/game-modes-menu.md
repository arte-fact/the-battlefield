# Game Modes & Skirmish Setup Menu

Two ways to start a battle, RTS-skirmish-page style, under the arcade
cabinet constraint (every menu fully operable with stick + 4 buttons):

```
MAIN MENU
├── ARCADE    — fixed config, random seed, straight into battle,
│               run is scored, top-10 board with 3-letter initials
└── SKIRMISH  — setup page (curated knobs) → battle, unscored
```

## Arcade mode

The cabinet experience: insert coin, play, chase the high score.

- Fixed rules = shipped `GameConfig` defaults; fresh random seed per run.
- **Run score**, accumulated in core during play:

  | Source | Points |
  |---|---|
  | Enemy killed by the player | 100 |
  | Zone captured while present (existing rep-FOV event) | 500 |
  | Peak authority | ×10 |
  | Survival | 1 / second |
  | Battle won | 5000 |

  Formula lives in one core function; values in `GameConfig` for tuning.
- **Flow**: death/victory → score tally screen → if top-10, 3-letter
  initials entry (stick up/down cycles A–Z per slot, attack button
  confirms — the classic) → scoreboard → main menu. Scoreboard also
  reachable from the main menu.
- **Persistence** follows the existing config pattern (wgpu-native
  already exposes get/set-config over wasm_bindgen to the page):
  core keeps `ScoreBoard { entries: Vec<ScoreEntry> }` with
  to/from JSON; hosts store it — web: localStorage via the page shim,
  native: a file next to the binary. Core never touches storage.

## Skirmish mode

A setup page with a **curated** knob set (not the whole GameConfig —
the dev panel already exposes that). Rows, RTS-style:

| Row | Values | Maps to |
|---|---|---|
| Map seed | number + REROLL | `setup_demo_battle_with_seed` |
| Your faction | Blue / Red | see work item below |
| Manpower (you) | 100–900, step 50 | `manpower_start` (per side) |
| Manpower (enemy) | 100–900, step 50 | idem — handicap by asymmetry |
| Army size cap | 20 / 35 / 50 | `max_units_per_faction` (perf audit headroom makes 50 safe) |
| Victory hold | 30 / 60 / 90 s | `victory_hold_time` |
| Zone bleed | Off / Low / Normal / High | `bleed_per_extra_zone` (0 / 0.1 / 0.25 / 0.5) |
| Starting authority | 0 / 25 / 50 | `authority` |

- **Navigation**: stick up/down selects a row, left/right adjusts the
  value, Attack = START, Dismiss = back. Touch: tap chevrons; the rows
  are large tap targets. One scheme, both inputs.
- Skirmish runs are unscored (configs aren't comparable); end screens
  return to the setup page with settings preserved (rematch-friendly).
- `SkirmishConfig` is a small struct applied onto `GameConfig` +
  battle-setup args at start; defaults = arcade values.

## UI construction

Built from the shared UI kit (no new assets needed):

- Setup page: SpecialPaper panel, SmallRibbons row labels, TinyRound
  blue/red buttons as chevrons, BigBlueButton START, gear icon
  (Icon_10, reserved earlier) as the Skirmish entry on the main menu.
- Scoreboard: RegularPaper panel, BigRibbons header, avatar portrait
  per entry (Avatars_01–25, picked by initials hash) + initials +
  score — finally using the avatar pack.
- Menu *logic* lives in core (`ui.rs` already has `ScreenLayout` /
  `ButtonAction`): a `MenuModel` (rows, values, focused index,
  adjust/confirm methods) that both renderers render — same
  mutualization pattern as the UI kit; only layout arithmetic stays
  per-renderer.

## State machine

`GameScreen` grows: `MainMenu → { Playing(Arcade) | SkirmishSetup →
Playing(Skirmish) } → PlayerDeath | GameWon | GameLost → (arcade only:
ScoreEntry? → ScoreBoard) → MainMenu`. Mode is a field on `Game` or
carried by the loop, not inferred.

## Work item: playable Red faction

Skirmish's "Your faction" row needs de-hardcoding: the player unit
spawns Blue and ~16 sites in setup.rs rep/zone events compare against
`Controlled(Faction::Blue)` literally. Parameterize on
`player_faction` (units/retinue already key off `player.faction`).
Ship the menu first with the row greyed to Blue if this drags.

## Roadmap (checkable)

- [ ] 1. Core score model: kill/capture/survival counters on `Game`,
  `RunScore` + total formula (+ config values), tests.
- [ ] 2. Core `MenuModel` + `SkirmishConfig` (rows, clamps, apply-to-
  GameConfig), `ScoreBoard` + initials entry model + JSON round-trip,
  tests (navigation, clamping, top-10 insertion, serde).
- [ ] 3. Screens: extend `GameScreen` + flows (arcade vs skirmish end
  screens), input routing (stick/keys/touch) through the shared model.
- [ ] 4. Rendering in both renderers via the UI kit (setup page,
  tally, initials, scoreboard); wgpu web verified headlessly with
  screenshots like the UI-kit pass.
- [ ] 5. Persistence shims: wasm_bindgen scoreboard get/set + page
  localStorage; native file. Config-panel pattern reused.
- [ ] 6. Playable Red (de-hardcode Blue in rep/zone events) — or ship
  greyed.
- [ ] 7. Verify: tests, clippy, fmt, probes unaffected (menus don't
  touch sim), GDD screens/UI sections, this roadmap.

## Decisions taken (flag if you disagree)

- Arcade scores only — skirmish is a sandbox, no board pollution.
- Curated skirmish knobs; the full config stays in the dev panel.
- 3-letter initials, top-10, local-only (no backend).
- Menus stick+4-button first; touch/mouse layered on the same model.

## Out of scope

- Online/shared leaderboards, attract mode, coin-op ceremony.
- Map size/shape options (one 128² map family for now).
- Unit-kind selection for the player (GDD defers it to
  meta-progression).
- Audio/settings menu (gear icon reserved, page comes later).
