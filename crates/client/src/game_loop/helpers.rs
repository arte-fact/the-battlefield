use battlefield_core::game::Game;
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
