use battlefield_sdl::game_loop::{GameLoop, GameLoopConfig, WINDOW_H, WINDOW_W};
use sdl2::render::TextureCreator;
use sdl2::video::WindowContext;

fn main() {
    env_logger::init();
    log::info!("The Battlefield -- SDL2 native starting up");

    let sdl = sdl2::init().expect("SDL2 init failed");
    let video = sdl.video().expect("SDL2 video init failed");
    let game_controller_subsystem = sdl.game_controller().expect("controller subsystem failed");

    let window = {
        let mut wb = video.window("The Battlefield", WINDOW_W, WINDOW_H);
        wb.resizable();
        wb.position_centered();
        wb.allow_highdpi();
        wb.build().expect("Window creation failed")
    };

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .expect("Canvas creation failed");

    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");

    let texture_creator: &'static TextureCreator<WindowContext> =
        Box::leak(Box::new(canvas.texture_creator()));

    // Compute native DPI scale
    let (output_w, _) = canvas.output_size().unwrap_or((WINDOW_W, WINDOW_H));
    let (logical_w, _) = canvas.window().size();
    let dpi = if logical_w > 0 {
        output_w as f64 / logical_w as f64
    } else {
        1.0
    };

    let event_pump = sdl.event_pump().expect("Event pump failed");

    let mut game_loop = GameLoop::new(
        canvas,
        texture_creator,
        event_pump,
        game_controller_subsystem,
        GameLoopConfig {
            dpi_scale: dpi,
            touch_dpr: dpi as f32,
            quit_on_escape: true,
            compute_dpi: true,
            profiling: std::env::var("PERF_PROFILE").is_ok(),
        },
    );

    loop {
        if !game_loop.step() {
            break;
        }
    }
    log::info!("Shutting down");
}
