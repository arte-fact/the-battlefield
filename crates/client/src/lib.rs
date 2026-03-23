#![allow(clippy::too_many_arguments)]

mod game_loop;
mod input;
mod renderer;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to init logger");

    log::info!("The Battlefield - Canvas 2D starting up");

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("game-canvas")
        .ok_or("no canvas element")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let dpr = window.device_pixel_ratio() as f32;
    let css_w = canvas.client_width() as f32;
    let css_h = canvas.client_height() as f32;
    let (canvas_w, canvas_h) = if css_w > 0.0 && css_h > 0.0 {
        ((css_w * dpr) as u32, (css_h * dpr) as u32)
    } else {
        ((960.0 * dpr) as u32, (640.0 * dpr) as u32)
    };
    canvas.set_width(canvas_w);
    canvas.set_height(canvas_h);

    let renderer = renderer::Canvas2dRenderer::new(&canvas, dpr as f64)?;

    let mut game_state = battlefield_core::game::Game::new(canvas_w as f32, canvas_h as f32);
    let initial_seed = (js_sys::Math::random() * u32::MAX as f64) as u32;
    game_state.setup_demo_battle_with_seed(initial_seed);

    game_loop::run(renderer, game_state, &canvas, initial_seed)?;

    Ok(())
}
