pub mod camera;
pub mod combat;
pub mod game;
pub mod grid;
pub mod input;
pub mod particle;
pub mod sprite;
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

    canvas.set_width(960);
    canvas.set_height(640);

    let canvas2d = renderer::Canvas2d::new(&canvas)?;

    let mut game_state = game::Game::new(960.0, 640.0);
    game_state.setup_demo_battle();

    let texture_manager = renderer::TextureManager::new();

    game_loop::run(canvas2d, game_state, texture_manager, &canvas)?;

    Ok(())
}
