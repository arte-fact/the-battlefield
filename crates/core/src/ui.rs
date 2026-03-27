//! Shared UI layout descriptors for all frontends.
//!
//! Defines WHAT to draw on overlay screens -- frontends decide HOW.
//! All offsets are in logical pixels, relative to screen center.

use crate::game::Game;

/// Generate a seed from the current system time.
pub fn generate_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(42)
}

/// Apply a UI button action to the game state.
///
/// Handles Play, Retry (same seed), and NewGame (fresh seed) transitions.
#[allow(clippy::too_many_arguments)]
pub fn handle_button_action(
    action: ButtonAction,
    screen: &mut GameScreen,
    game: &mut Game,
    seed: &mut u32,
    player_was_alive: &mut bool,
    viewport_w: u32,
    viewport_h: u32,
    source: &str,
) {
    match action {
        ButtonAction::Play => {
            *screen = GameScreen::Playing;
            log::info!("Game started ({source})");
        }
        ButtonAction::Retry => {
            *game = Game::new(viewport_w as f32, viewport_h as f32);
            game.setup_demo_battle_with_seed(*seed);
            *screen = GameScreen::Playing;
            *player_was_alive = true;
            log::info!("Retrying with seed {} ({source})", *seed);
        }
        ButtonAction::NewGame => {
            *seed = generate_seed();
            *game = Game::new(viewport_w as f32, viewport_h as f32);
            game.setup_demo_battle_with_seed(*seed);
            *screen = GameScreen::Playing;
            *player_was_alive = true;
            log::info!("New game with seed {} ({source})", *seed);
        }
    }
}

/// Which screen the game is currently showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameScreen {
    MainMenu,
    Playing,
    PlayerDeath,
    GameWon,
    GameLost,
}

/// Action triggered by clicking a UI button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    Play,
    Retry,
    NewGame,
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
    pub label: &'static str,
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
pub fn main_menu_layout() -> ScreenLayout {
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
        subtitle: None,
        buttons: vec![ButtonDesc {
            label: "PLAY",
            offset_x: 0.0,
            offset_y: 40.0,
            w: 200.0,
            h: 60.0,
            style: ButtonStyle::Blue,
            action: ButtonAction::Play,
        }],
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
                label: "RETRY",
                offset_x: -85.0,
                offset_y: 41.0,
                w: 150.0,
                h: 50.0,
                style: ButtonStyle::Red,
                action: ButtonAction::Retry,
            },
            ButtonDesc {
                label: "NEW GAME",
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
                label: retry_label,
                offset_x: -85.0,
                offset_y: 55.0,
                w: 150.0,
                h: 50.0,
                style: ButtonStyle::Red,
                action: ButtonAction::Retry,
            },
            ButtonDesc {
                label: "NEW GAME",
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
