pub mod animation;
pub mod autotile;
pub mod camera;
pub mod combat;
pub mod game;
pub mod grid;
pub mod input;
pub mod particle;
pub mod sprite;
pub mod mapgen;
pub mod turn;
pub mod unit;

#[cfg(target_arch = "wasm32")]
mod game_loop;
#[cfg(target_arch = "wasm32")]
mod renderer;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
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

    // DPR-aware canvas sizing: scale backing store for sharp rendering on high-DPI screens
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

    let canvas2d = renderer::Canvas2d::new(&canvas)?;

    let mut game_state = game::Game::new(canvas_w as f32, canvas_h as f32);
    game_state.setup_demo_battle();

    let texture_manager = renderer::TextureManager::new();

    game_loop::run(canvas2d, game_state, texture_manager, &canvas)?;

    Ok(())
}
