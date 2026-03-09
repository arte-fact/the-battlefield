pub mod sprite;

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

    log::info!("The Battlefield - starting up");

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("game-canvas")
        .ok_or("no canvas element")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    canvas.set_width(960);
    canvas.set_height(640);

    let gpu = renderer::Gpu::new(&canvas).await?;
    let sprite_sheet = sprite::SpriteSheet::from_url(
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Idle.png",
        192,
        192,
        8,
    )
    .await?;

    let sprite_renderer = renderer::SpriteRenderer::new(&gpu, &sprite_sheet)?;

    game_loop::run(gpu, sprite_renderer, sprite_sheet)?;

    Ok(())
}
