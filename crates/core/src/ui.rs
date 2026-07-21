//! Shared UI layout descriptors for all frontends.
//!
//! Defines WHAT to draw on overlay screens -- frontends decide HOW.
//! All offsets are in logical pixels, relative to screen center.

use crate::config::GameConfig;
use crate::game::Game;
use serde::{Deserialize, Serialize};

/// Derive a fresh seed from the previous one (splitmix32 step).
/// Core stays clock-free; hosts inject time-based entropy at startup.
pub fn next_seed(state: u32) -> u32 {
    let mut z = state.wrapping_add(0x9E37_79B9);
    z = (z ^ (z >> 16)).wrapping_mul(0x21F0_AAAD);
    z = (z ^ (z >> 15)).wrapping_mul(0x735A_2D97);
    z ^ (z >> 15)
}

/// Apply a UI button action to game/screen/menu state.
#[allow(clippy::too_many_arguments)]
pub fn handle_button_action(
    action: ButtonAction,
    screen: &mut GameScreen,
    game: &mut Game,
    seed: &mut u32,
    player_was_alive: &mut bool,
    viewport_w: u32,
    viewport_h: u32,
    ui: &mut UiState,
    source: &str,
) {
    match action {
        ButtonAction::Play => {
            *seed = next_seed(*seed);
            ui.mode = GameMode::Arcade;
            start_battle(
                game,
                screen,
                player_was_alive,
                viewport_w,
                viewport_h,
                *seed,
                ui,
            );
            log::info!("Arcade run, seed {} ({source})", *seed);
        }
        ButtonAction::Retry => {
            start_battle(
                game,
                screen,
                player_was_alive,
                viewport_w,
                viewport_h,
                *seed,
                ui,
            );
            log::info!("Retrying with seed {} ({source})", *seed);
        }
        ButtonAction::NewGame => {
            *seed = next_seed(*seed);
            start_battle(
                game,
                screen,
                player_was_alive,
                viewport_w,
                viewport_h,
                *seed,
                ui,
            );
            log::info!("New game with seed {} ({source})", *seed);
        }
        ButtonAction::OpenSkirmish => *screen = GameScreen::SkirmishSetup,
        ButtonAction::OpenScores => *screen = GameScreen::ScoreBoard,
        ButtonAction::Back => *screen = GameScreen::MainMenu,
        ButtonAction::StartSkirmish => {
            ui.mode = GameMode::Skirmish;
            *seed = ui.skirmish.seed;
            start_battle(
                game,
                screen,
                player_was_alive,
                viewport_w,
                viewport_h,
                *seed,
                ui,
            );
            log::info!("Skirmish, seed {} ({source})", *seed);
        }
        ButtonAction::AdjustRow(row) => {
            ui.focused_row = row as usize;
            ui.skirmish
                .adjust(row as usize, 1, next_seed(ui.skirmish.seed));
        }
        ButtonAction::ConfirmInitials => {
            if let Some(score) = ui.pending_score.take() {
                ui.scoreboard.insert(ui.initials.text(), score.total());
            }
            *screen = GameScreen::ScoreBoard;
        }
    }
}

/// Reset and start a battle with the active mode's configuration.
fn start_battle(
    game: &mut Game,
    screen: &mut GameScreen,
    player_was_alive: &mut bool,
    viewport_w: u32,
    viewport_h: u32,
    seed: u32,
    ui: &UiState,
) {
    *game = Game::new(viewport_w as f32, viewport_h as f32);
    match ui.mode {
        GameMode::Skirmish => ui.skirmish.apply(&mut game.config),
        GameMode::Arcade => {
            game.config.enemy_count = ui.scoreboard.arcade_level;
            game.config.playable_size = auto_playable_size(ui.scoreboard.arcade_level);
        }
    }
    game.begin_async_setup(seed);
    *screen = GameScreen::Loading;
    *player_was_alive = true;
}

/// Pump budgeted setup from the Loading screen. Hosts call `Game::setup_step`
/// in a frame-time loop, then this once it reports completion.
pub fn finish_loading(game: &mut Game, screen: &mut GameScreen, ui: &UiState) {
    if ui.mode == GameMode::Skirmish {
        ui.skirmish.apply_to_game(game);
    }
    *screen = GameScreen::Playing;
}

/// Which screen the game is currently showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameScreen {
    MainMenu,
    SkirmishSetup,
    Loading,
    Playing,
    PlayerDeath,
    GameWon,
    GameLost,
    ScoreEntry,
    ScoreBoard,
}

/// Action triggered by clicking a UI button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    Play,
    Retry,
    NewGame,
    OpenSkirmish,
    OpenScores,
    Back,
    StartSkirmish,
    AdjustRow(u8),
    ConfirmInitials,
}

/// Button visual style.
#[derive(Clone, Copy, Debug)]
pub enum ButtonStyle {
    Blue,
    Red,
}

/// A text element in a screen layout.
#[derive(Clone, Debug)]
pub struct TextElement {
    pub text: String,
    /// Offset from screen center in logical pixels.
    pub offset_x: f64,
    pub offset_y: f64,
    pub size: f64,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub bold: bool,
    pub shadow: bool,
}

/// A button in a screen layout.
#[derive(Clone, Debug)]
pub struct ButtonDesc {
    pub label: String,
    /// Center offset from screen center in logical pixels.
    pub offset_x: f64,
    pub offset_y: f64,
    pub w: f64,
    pub h: f64,
    pub style: ButtonStyle,
    pub action: ButtonAction,
}

/// Full screen overlay descriptor.
#[derive(Clone, Debug)]
pub struct ScreenLayout {
    /// Background tint color (RGBA).
    pub overlay: (u8, u8, u8, u8),
    /// Panel size in logical pixels (w, h). Centered on screen.
    pub panel_size: Option<(f64, f64)>,
    /// Ribbon behind title: (color_row, offset_y_from_panel_top, width, height).
    pub title_ribbon: Option<(u32, f64, f64, f64)>,
    pub title: Option<TextElement>,
    pub subtitle: Option<TextElement>,
    pub buttons: Vec<ButtonDesc>,
    pub hints: Vec<TextElement>,
}

/// Build the main menu layout.
///
/// Panel: 500x350, centered. Ribbon at panel_top+30, title centered on ribbon.
/// PLAY button below center. Two hint lines at the bottom.
pub fn main_menu_layout(ui: &UiState) -> ScreenLayout {
    // panel_y = cy - 175, ribbon_y = cy - 145, title_y = cy - 109
    // btn top = cy + 10, btn center = cy + 40
    // hint1 = cy + 96, hint2 = cy + 116
    ScreenLayout {
        overlay: (0, 0, 0, 190),
        panel_size: Some((560.0, 350.0)),
        title_ribbon: Some((0, 30.0, 520.0, 72.0)),
        title: Some(TextElement {
            text: "THE BATTLEFIELD".into(),
            offset_x: 0.0,
            offset_y: -109.0,
            size: 48.0,
            r: 255,
            g: 215,
            b: 0,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: Some(TextElement {
            text: format!("ARCADE LADDER: 1v{}", ui.scoreboard.arcade_level),
            offset_x: 0.0,
            offset_y: -62.0,
            size: 16.0,
            r: 230,
            g: 230,
            b: 230,
            a: 220,
            bold: false,
            shadow: false,
        }),
        buttons: vec![
            ButtonDesc {
                label: "ARCADE".into(),
                offset_x: 0.0,
                offset_y: 5.0,
                w: 220.0,
                h: 54.0,
                style: ButtonStyle::Blue,
                action: ButtonAction::Play,
            },
            ButtonDesc {
                label: "SKIRMISH".into(),
                offset_x: 0.0,
                offset_y: 68.0,
                w: 220.0,
                h: 54.0,
                style: ButtonStyle::Red,
                action: ButtonAction::OpenSkirmish,
            },
            ButtonDesc {
                label: "SCORES".into(),
                offset_x: 0.0,
                offset_y: 128.0,
                w: 220.0,
                h: 44.0,
                style: ButtonStyle::Blue,
                action: ButtonAction::OpenScores,
            },
        ],
        hints: vec![],
    }
}

/// Build the death screen layout.
///
/// Panel: 460x300, centered. Red ribbon at panel_top+24, title on ribbon.
/// RETRY + NEW GAME buttons. Hint line below.
pub fn death_layout() -> ScreenLayout {
    // panel_y = cy - 150, ribbon_y = cy - 126, title_y = cy - 92
    // btn_y (top) = cy + 16, btn center = cy + 41
    // hint_y = cy + 86
    ScreenLayout {
        overlay: (80, 0, 0, 150),
        panel_size: Some((460.0, 300.0)),
        title_ribbon: Some((1, 24.0, 320.0, 68.0)),
        title: Some(TextElement {
            text: "YOU DIED".into(),
            offset_x: 0.0,
            offset_y: -92.0,
            size: 48.0,
            r: 204,
            g: 34,
            b: 34,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: None,
        buttons: vec![
            ButtonDesc {
                label: "RETRY".into(),
                offset_x: -85.0,
                offset_y: 41.0,
                w: 150.0,
                h: 50.0,
                style: ButtonStyle::Red,
                action: ButtonAction::Retry,
            },
            ButtonDesc {
                label: "NEW GAME".into(),
                offset_x: 85.0,
                offset_y: 41.0,
                w: 150.0,
                h: 50.0,
                style: ButtonStyle::Blue,
                action: ButtonAction::NewGame,
            },
        ],
        hints: vec![],
    }
}

/// Build the victory/defeat result screen layout.
///
/// Panel: 480x340, centered. Blue/red ribbon at panel_top+24, title on ribbon.
/// Subtitle below ribbon. RETRY/REPLAY + NEW GAME buttons. Hint line at bottom.
pub fn result_layout(is_victory: bool) -> ScreenLayout {
    // panel_y = cy - 170, ribbon_y = cy - 146, title_y = cy - 112
    // subtitle_y = cy - 54
    // btn_y (top) = cy + 30, btn center = cy + 55
    // hint_y = cy + 100
    let (title, tr, tg, tb) = if is_victory {
        ("VICTORY", 78u8, 168, 255)
    } else {
        ("DEFEAT", 255, 85, 85)
    };
    let ribbon_row = if is_victory { 0 } else { 1 };
    let subtitle = if is_victory {
        "Your forces have triumphed!"
    } else {
        "Your army has been defeated."
    };
    let retry_label = if is_victory { "REPLAY" } else { "RETRY" };

    ScreenLayout {
        overlay: if is_victory {
            (0, 30, 60, 150)
        } else {
            (40, 0, 0, 150)
        },
        panel_size: Some((480.0, 340.0)),
        title_ribbon: Some((ribbon_row, 24.0, 340.0, 68.0)),
        title: Some(TextElement {
            text: title.into(),
            offset_x: 0.0,
            offset_y: -112.0,
            size: 48.0,
            r: tr,
            g: tg,
            b: tb,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: Some(TextElement {
            text: subtitle.into(),
            offset_x: 0.0,
            offset_y: -54.0,
            size: 16.0,
            r: 255,
            g: 255,
            b: 255,
            a: 180,
            bold: false,
            shadow: false,
        }),
        buttons: vec![
            ButtonDesc {
                label: retry_label.into(),
                offset_x: -85.0,
                offset_y: 55.0,
                w: 150.0,
                h: 50.0,
                style: ButtonStyle::Red,
                action: ButtonAction::Retry,
            },
            ButtonDesc {
                label: "NEW GAME".into(),
                offset_x: 85.0,
                offset_y: 55.0,
                w: 150.0,
                h: 50.0,
                style: ButtonStyle::Blue,
                action: ButtonAction::NewGame,
            },
        ],
        hints: vec![],
    }
}

// ── Game modes, skirmish setup, scoring ─────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GameMode {
    #[default]
    Arcade,
    Skirmish,
}

/// Curated skirmish knobs, applied onto GameConfig at battle start.
#[derive(Clone, Copy, Debug)]
pub struct SkirmishConfig {
    pub seed: u32,
    pub enemies: u8,
    pub map_size_idx: usize,
    pub manpower_you: f32,
    pub manpower_enemy: f32,
    pub army_cap: usize,
    pub victory_hold: f32,
    pub bleed_idx: usize,
    pub start_authority: f32,
}

pub const BLEED_LEVELS: [(&str, f32); 4] =
    [("OFF", 0.0), ("LOW", 0.1), ("NORMAL", 0.25), ("HIGH", 0.5)];

/// MAP SIZE row values; 0 scales with the enemy count.
pub const MAP_SIZES: [(&str, u32); 4] = [
    ("AUTO", 0),
    ("LARGE", 384),
    ("HUGE", 512),
    ("COLOSSAL", 1024),
];

/// Default playable size for a given enemy count (AUTO map size).
pub fn auto_playable_size(enemies: u8) -> u32 {
    160 + 32 * enemies.saturating_sub(1) as u32
}

impl Default for SkirmishConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            enemies: 1,
            map_size_idx: 0,
            manpower_you: 300.0,
            manpower_enemy: 300.0,
            army_cap: 35,
            victory_hold: 60.0,
            bleed_idx: 2,
            start_authority: 0.0,
        }
    }
}

impl SkirmishConfig {
    pub const ROWS: usize = 9;

    pub fn apply(&self, cfg: &mut GameConfig) {
        cfg.enemy_count = self.enemies;
        cfg.playable_size = match MAP_SIZES[self.map_size_idx].1 {
            0 => auto_playable_size(self.enemies),
            fixed => fixed,
        };
        cfg.max_units_per_faction = self.army_cap;
        cfg.victory_hold_time = self.victory_hold;
        cfg.bleed_per_extra_zone = BLEED_LEVELS[self.bleed_idx].1;
    }

    /// Post-setup per-side values that GameConfig cannot express.
    pub fn apply_to_game(&self, game: &mut Game) {
        game.manpower = [
            self.manpower_you,
            self.manpower_enemy,
            self.manpower_enemy,
            self.manpower_enemy,
        ];
        game.authority = self.start_authority;
    }

    pub fn row_label(row: usize) -> &'static str {
        [
            "MAP SEED",
            "ENEMIES",
            "MAP SIZE",
            "MANPOWER (YOU)",
            "MANPOWER (ENEMY)",
            "ARMY SIZE CAP",
            "VICTORY HOLD",
            "ZONE BLEED",
            "STARTING AUTHORITY",
        ][row]
    }

    pub fn row_value(&self, row: usize) -> String {
        match row {
            0 => format!("{}", self.seed),
            1 => format!("1v{}", self.enemies),
            2 => MAP_SIZES[self.map_size_idx].0.to_string(),
            3 => format!("{}", self.manpower_you as u32),
            4 => format!("{}", self.manpower_enemy as u32),
            5 => format!("{}", self.army_cap),
            6 => format!("{}s", self.victory_hold as u32),
            7 => BLEED_LEVELS[self.bleed_idx].0.to_string(),
            8 => format!("{}", self.start_authority as u32),
            _ => String::new(),
        }
    }

    /// Adjust a row by direction (+1/-1). `entropy` feeds seed rerolls.
    pub fn adjust(&mut self, row: usize, dir: i32, entropy: u32) {
        let step =
            |v: f32, d: i32, step: f32, min: f32, max: f32| (v + d as f32 * step).clamp(min, max);
        match row {
            0 => self.seed = entropy,
            1 => self.enemies = ((self.enemies as i32 - 1 + dir).rem_euclid(3) + 1) as u8,
            2 => {
                self.map_size_idx =
                    (self.map_size_idx as i32 + dir).rem_euclid(MAP_SIZES.len() as i32) as usize;
            }
            3 => self.manpower_you = step(self.manpower_you, dir, 50.0, 100.0, 900.0),
            4 => self.manpower_enemy = step(self.manpower_enemy, dir, 50.0, 100.0, 900.0),
            5 => {
                let caps = [20usize, 35, 50];
                let i = caps.iter().position(|&c| c == self.army_cap).unwrap_or(1);
                self.army_cap = caps[(i as i32 + dir).rem_euclid(3) as usize];
            }
            6 => {
                let holds = [30.0f32, 60.0, 90.0];
                let i = holds
                    .iter()
                    .position(|&h| h == self.victory_hold)
                    .unwrap_or(1);
                self.victory_hold = holds[(i as i32 + dir).rem_euclid(3) as usize];
            }
            7 => {
                self.bleed_idx =
                    (self.bleed_idx as i32 + dir).rem_euclid(BLEED_LEVELS.len() as i32) as usize;
            }
            8 => self.start_authority = step(self.start_authority, dir, 25.0, 0.0, 50.0),
            _ => {}
        }
    }
}

// ── Score model ─────────────────────────────────────────────────────────

pub const SCORE_PER_KILL: u32 = 100;
pub const SCORE_PER_ZONE: u32 = 500;
pub const SCORE_AUTHORITY_MULT: u32 = 10;
pub const SCORE_PER_SURVIVAL_SEC: u32 = 1;
pub const SCORE_VICTORY_BONUS: u32 = 5000;

#[derive(Clone, Copy, Debug, Default)]
pub struct RunScore {
    pub kills: u32,
    pub zone_caps: u32,
    pub peak_authority: u32,
    pub survival_secs: u32,
    pub victory: bool,
    /// Enemy count of the run; the total scales with it.
    pub enemies: u32,
}

impl RunScore {
    pub fn from_game(game: &Game, victory: bool) -> Self {
        Self {
            kills: game.score_kills,
            zone_caps: game.score_zone_caps,
            peak_authority: game.score_peak_authority as u32,
            survival_secs: game.survival_secs as u32,
            victory,
            enemies: game.config.enemy_count as u32,
        }
    }

    pub fn total(&self) -> u32 {
        (self.kills * SCORE_PER_KILL
            + self.zone_caps * SCORE_PER_ZONE
            + self.peak_authority * SCORE_AUTHORITY_MULT
            + self.survival_secs * SCORE_PER_SURVIVAL_SEC
            + if self.victory { SCORE_VICTORY_BONUS } else { 0 })
            * self.enemies.max(1)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoreEntry {
    pub initials: String,
    pub score: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreBoard {
    pub entries: Vec<ScoreEntry>,
    /// Arcade ladder: current enemy count (1..=3). Climbs on victory,
    /// resets on defeat. Persisted with the scores.
    #[serde(default = "default_arcade_level")]
    pub arcade_level: u8,
}

fn default_arcade_level() -> u8 {
    1
}

impl Default for ScoreBoard {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            arcade_level: 1,
        }
    }
}

impl ScoreBoard {
    pub const CAP: usize = 10;

    /// Rank the score would take (0-based), if it makes the board.
    pub fn rank_for(&self, score: u32) -> Option<usize> {
        let rank = self.entries.iter().take_while(|e| e.score >= score).count();
        (rank < Self::CAP).then_some(rank)
    }

    pub fn insert(&mut self, initials: String, score: u32) -> Option<usize> {
        let rank = self.rank_for(score)?;
        self.entries.insert(rank, ScoreEntry { initials, score });
        self.entries.truncate(Self::CAP);
        Some(rank)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

impl UiState {
    /// Advance the arcade ladder after a finished arcade run.
    pub fn finish_arcade_run(&mut self, victory: bool) {
        self.scoreboard.arcade_level = if victory {
            (self.scoreboard.arcade_level + 1).min(3)
        } else {
            1
        };
    }
}

/// Arcade 3-letter initials entry: stick up/down cycles A–Z per slot.
#[derive(Clone, Copy, Debug, Default)]
pub struct InitialsEntry {
    pub letters: [u8; 3],
    pub slot: usize,
}

impl InitialsEntry {
    pub fn cycle(&mut self, dir: i32) {
        let l = &mut self.letters[self.slot];
        *l = ((*l as i32 + dir).rem_euclid(26)) as u8;
    }

    pub fn move_slot(&mut self, dir: i32) {
        self.slot = (self.slot as i32 + dir).rem_euclid(3) as usize;
    }

    pub fn text(&self) -> String {
        self.letters.iter().map(|&l| (b'A' + l) as char).collect()
    }
}

/// Cross-screen menu/score state owned by the frontend game loop.
#[derive(Default)]
pub struct UiState {
    pub mode: GameMode,
    pub skirmish: SkirmishConfig,
    pub focused_row: usize,
    pub scoreboard: ScoreBoard,
    pub initials: InitialsEntry,
    pub pending_score: Option<RunScore>,
    /// Mirror of `Game::setup_progress`, refreshed by hosts each Loading frame.
    pub loading_progress: f32,
}

/// Loading screen: the title ribbon doubles as the progress bar.
pub fn loading_layout(progress: f32) -> ScreenLayout {
    let p = progress.clamp(0.0, 1.0) as f64;
    ScreenLayout {
        overlay: (0, 0, 0, 190),
        panel_size: Some((560.0, 200.0)),
        title_ribbon: Some((0, 78.0, 40.0 + 480.0 * p, 44.0)),
        title: Some(TextElement {
            text: "RAISING THE BANNERS".into(),
            offset_x: 0.0,
            offset_y: -60.0,
            size: 30.0,
            r: 255,
            g: 215,
            b: 0,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: Some(TextElement {
            text: format!("{:.0}%", p * 100.0),
            offset_x: 0.0,
            offset_y: 0.0,
            size: 20.0,
            r: 230,
            g: 230,
            b: 230,
            a: 255,
            bold: false,
            shadow: false,
        }),
        buttons: vec![],
        hints: vec![],
    }
}

// ── Layout builders for the new screens ─────────────────────────────────

const ROW_H: f64 = 34.0;

pub fn skirmish_layout(ui: &UiState) -> ScreenLayout {
    let mut buttons = Vec::new();
    let top = -140.0;
    for row in 0..SkirmishConfig::ROWS {
        let focused = row == ui.focused_row;
        buttons.push(ButtonDesc {
            label: format!(
                "{} {:<19} < {} >",
                if focused { ">" } else { " " },
                SkirmishConfig::row_label(row),
                ui.skirmish.row_value(row)
            ),
            offset_x: 0.0,
            offset_y: top + row as f64 * ROW_H,
            w: 440.0,
            h: 30.0,
            style: if focused {
                ButtonStyle::Red
            } else {
                ButtonStyle::Blue
            },
            action: ButtonAction::AdjustRow(row as u8),
        });
    }
    buttons.push(ButtonDesc {
        label: "START".into(),
        offset_x: -85.0,
        offset_y: 185.0,
        w: 150.0,
        h: 50.0,
        style: ButtonStyle::Blue,
        action: ButtonAction::StartSkirmish,
    });
    buttons.push(ButtonDesc {
        label: "BACK".into(),
        offset_x: 85.0,
        offset_y: 185.0,
        w: 150.0,
        h: 50.0,
        style: ButtonStyle::Red,
        action: ButtonAction::Back,
    });
    ScreenLayout {
        overlay: (0, 0, 0, 190),
        panel_size: Some((560.0, 490.0)),
        title_ribbon: Some((0, 22.0, 380.0, 60.0)),
        title: Some(TextElement {
            text: "SKIRMISH".into(),
            offset_x: 0.0,
            offset_y: -193.0,
            size: 34.0,
            r: 255,
            g: 215,
            b: 0,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: None,
        buttons,
        hints: vec![],
    }
}

pub fn score_entry_layout(ui: &UiState) -> ScreenLayout {
    let total = ui.pending_score.map(|s| s.total()).unwrap_or(0);
    let mut text = String::new();
    for (i, ch) in ui.initials.text().chars().enumerate() {
        if i == ui.initials.slot {
            text.push('[');
            text.push(ch);
            text.push(']');
        } else {
            text.push(' ');
            text.push(ch);
            text.push(' ');
        }
    }
    ScreenLayout {
        overlay: (0, 20, 40, 190),
        panel_size: Some((460.0, 300.0)),
        title_ribbon: Some((2, 24.0, 340.0, 64.0)),
        title: Some(TextElement {
            text: "HIGH SCORE!".into(),
            offset_x: 0.0,
            offset_y: -92.0,
            size: 36.0,
            r: 255,
            g: 215,
            b: 0,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: Some(TextElement {
            text: format!("{total} PTS — ENTER YOUR NAME"),
            offset_x: 0.0,
            offset_y: -40.0,
            size: 16.0,
            r: 255,
            g: 255,
            b: 255,
            a: 200,
            bold: false,
            shadow: false,
        }),
        buttons: vec![ButtonDesc {
            label: "OK".into(),
            offset_x: 0.0,
            offset_y: 95.0,
            w: 150.0,
            h: 50.0,
            style: ButtonStyle::Blue,
            action: ButtonAction::ConfirmInitials,
        }],
        hints: vec![TextElement {
            text,
            offset_x: 0.0,
            offset_y: 15.0,
            size: 44.0,
            r: 255,
            g: 255,
            b: 255,
            a: 255,
            bold: true,
            shadow: true,
        }],
    }
}

pub fn scoreboard_layout(ui: &UiState) -> ScreenLayout {
    let mut hints = Vec::new();
    if ui.scoreboard.entries.is_empty() {
        hints.push(TextElement {
            text: "NO SCORES YET — PLAY ARCADE!".into(),
            offset_x: 0.0,
            offset_y: 0.0,
            size: 18.0,
            r: 255,
            g: 255,
            b: 255,
            a: 180,
            bold: false,
            shadow: false,
        });
    }
    for (i, e) in ui.scoreboard.entries.iter().enumerate() {
        hints.push(TextElement {
            text: format!("{:>2}. {}   {:>7}", i + 1, e.initials, e.score),
            offset_x: 0.0,
            offset_y: -105.0 + i as f64 * 26.0,
            size: 18.0,
            r: 255,
            g: if i == 0 { 215 } else { 255 },
            b: if i == 0 { 0 } else { 255 },
            a: 235,
            bold: i == 0,
            shadow: false,
        });
    }
    ScreenLayout {
        overlay: (0, 0, 0, 200),
        panel_size: Some((460.0, 430.0)),
        title_ribbon: Some((0, 22.0, 340.0, 60.0)),
        title: Some(TextElement {
            text: "HIGH SCORES".into(),
            offset_x: 0.0,
            offset_y: -163.0,
            size: 32.0,
            r: 255,
            g: 215,
            b: 0,
            a: 255,
            bold: true,
            shadow: false,
        }),
        subtitle: None,
        buttons: vec![ButtonDesc {
            label: "BACK".into(),
            offset_x: 0.0,
            offset_y: 175.0,
            w: 150.0,
            h: 50.0,
            style: ButtonStyle::Blue,
            action: ButtonAction::Back,
        }],
        hints,
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn scoreboard_top10_insertion() {
        let mut b = ScoreBoard::default();
        for i in 0..12u32 {
            b.insert(format!("P{i:02}"), i * 100);
        }
        assert_eq!(b.entries.len(), 10);
        assert_eq!(b.entries[0].score, 1100);
        assert!(b.rank_for(50).is_none());
        assert_eq!(b.rank_for(99999), Some(0));
        let json = b.to_json();
        let back = ScoreBoard::from_json(&json).unwrap();
        assert_eq!(back.entries, b.entries);
    }

    #[test]
    fn skirmish_adjust_clamps_and_cycles() {
        let mut c = SkirmishConfig::default();
        for _ in 0..30 {
            c.adjust(3, 1, 0);
        }
        assert_eq!(c.manpower_you, 900.0);
        c.adjust(7, 1, 0);
        assert_eq!(c.bleed_idx, 3);
        c.adjust(7, 1, 0);
        assert_eq!(c.bleed_idx, 0);
        c.adjust(0, 1, 777);
        assert_eq!(c.seed, 777);
        let mut cfg = GameConfig::default();
        c.adjust(6, -1, 0);
        c.apply(&mut cfg);
        assert_eq!(cfg.victory_hold_time, 30.0);
        assert_eq!(cfg.bleed_per_extra_zone, 0.0);
    }

    #[test]
    fn skirmish_enemies_and_map_size_rows() {
        let mut c = SkirmishConfig::default();
        let mut cfg = GameConfig::default();
        c.apply(&mut cfg);
        assert_eq!(cfg.enemy_count, 1);
        assert_eq!(cfg.playable_size, 160);
        c.adjust(1, 1, 0);
        c.adjust(1, 1, 0);
        c.apply(&mut cfg);
        assert_eq!(cfg.enemy_count, 3);
        assert_eq!(cfg.playable_size, auto_playable_size(3));
        c.adjust(1, 1, 0); // wraps back to 1v1
        assert_eq!(c.enemies, 1);
        c.adjust(2, 1, 0);
        c.apply(&mut cfg);
        assert_eq!(cfg.playable_size, 384);
        c.adjust(2, 1, 0);
        c.apply(&mut cfg);
        assert_eq!(cfg.playable_size, 512);
        c.adjust(2, 1, 0);
        c.apply(&mut cfg);
        assert_eq!(cfg.playable_size, 1024);
    }

    #[test]
    fn arcade_ladder_climbs_and_resets() {
        let mut ui = UiState::default();
        ui.scoreboard.arcade_level = 1;
        ui.finish_arcade_run(true);
        assert_eq!(ui.scoreboard.arcade_level, 2);
        ui.finish_arcade_run(true);
        ui.finish_arcade_run(true);
        assert_eq!(ui.scoreboard.arcade_level, 3, "caps at 1v3");
        ui.finish_arcade_run(false);
        assert_eq!(ui.scoreboard.arcade_level, 1, "defeat resets the ladder");
        // Level survives the scores round-trip; old saves default to 1.
        ui.scoreboard.arcade_level = 3;
        let back = ScoreBoard::from_json(&ui.scoreboard.to_json()).unwrap();
        assert_eq!(back.arcade_level, 3);
        let old = ScoreBoard::from_json("{\"entries\":[]}").unwrap();
        assert_eq!(old.arcade_level, 1);
    }

    #[test]
    fn initials_cycle_and_text() {
        let mut e = InitialsEntry::default();
        e.cycle(-1);
        assert_eq!(e.text(), "ZAA");
        e.move_slot(1);
        e.cycle(2);
        assert_eq!(e.text(), "ZCA");
    }

    #[test]
    fn run_score_total() {
        let mut s = RunScore {
            kills: 3,
            zone_caps: 2,
            peak_authority: 40,
            survival_secs: 100,
            victory: true,
            enemies: 1,
        };
        assert_eq!(s.total(), 300 + 1000 + 400 + 100 + 5000);
        s.enemies = 3;
        assert_eq!(s.total(), (300 + 1000 + 400 + 100 + 5000) * 3);
    }
}
