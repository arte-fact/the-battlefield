use crate::game::Game;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Cached HUD DOM elements.
pub(super) struct HudElements {
    hp_bar_fill: web_sys::HtmlElement,
}

impl HudElements {
    pub(super) fn from_document(doc: &web_sys::Document) -> Option<Self> {
        let hp_bar_fill = doc
            .get_element_by_id("hp-bar-fill")?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()?;
        Some(Self { hp_bar_fill })
    }

    pub(super) fn update(&self, game: &Game) {
        if let Some(player) = game.player_unit() {
            let ratio = player.hp as f32 / player.stats.max_hp as f32;
            let pct = format!("{}%", (ratio * 100.0) as u32);
            let _ = self.hp_bar_fill.style().set_property("width", &pct);

            let color = if ratio > 0.5 {
                "#4caf50"
            } else if ratio > 0.25 {
                "#ff9800"
            } else {
                "#f44336"
            };
            let _ = self.hp_bar_fill.style().set_property("background", color);
        } else {
            let _ = self.hp_bar_fill.style().set_property("width", "0%");
        }
    }
}

/// Trigger haptic feedback (vibration) if supported.
pub(super) fn haptic(duration_ms: u32) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let nav_js: &JsValue = navigator.as_ref();
        if let Ok(vibrate_fn) = js_sys::Reflect::get(nav_js, &JsValue::from_str("vibrate")) {
            if vibrate_fn.is_function() {
                let _ =
                    js_sys::Function::from(vibrate_fn).call1(nav_js, &JsValue::from(duration_ms));
            }
        }
    }
}

/// Compute a wave-gated animation frame.
///
/// Uses a sine wave that sweeps across the grid to decide whether to animate.
/// When the wave is active at (gx, gy), returns a cycling frame index at 10 FPS;
/// otherwise returns frame 0 (idle).
pub(super) fn compute_wave_frame(
    elapsed: f64,
    gx: u32,
    gy: u32,
    frame_count: u32,
    speed: f64,
) -> u32 {
    let wave_pos =
        elapsed * speed + gx as f64 * 0.06 + gy as f64 * 0.04 + (gx ^ gy) as f64 * 0.01;
    if (wave_pos * std::f64::consts::TAU).sin() > 0.3 {
        ((elapsed * 10.0) as u32) % frame_count
    } else {
        0
    }
}
