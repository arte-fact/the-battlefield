mod emscripten;

use battlefield_sdl::game_loop::{GameLoop, GameLoopConfig};
use sdl2::render::TextureCreator;
use sdl2::video::WindowContext;

fn main() {
    log::info!("The Battlefield -- SDL2 emscripten starting up");

    let sdl = sdl2::init().expect("SDL2 init failed");
    let video = sdl.video().expect("SDL2 video init failed");
    let game_controller_subsystem = sdl.game_controller().expect("controller subsystem failed");

    let (init_w, init_h, em_dpr) = emscripten::viewport_size_device_pixels();

    let window = {
        let mut wb = video.window("The Battlefield", init_w, init_h);
        wb.resizable();
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

    let event_pump = sdl.event_pump().expect("Event pump failed");

    let game_loop = GameLoop::new(
        canvas,
        texture_creator,
        event_pump,
        game_controller_subsystem,
        GameLoopConfig {
            dpi_scale: 1.0,
            touch_dpr: em_dpr as f32,
            quit_on_escape: false,
            compute_dpi: false,
            profiling: false,
        },
    );

    let raw = Box::into_raw(Box::new(game_loop));
    unsafe {
        emscripten::emscripten_set_main_loop_arg(
            em_frame_callback,
            raw as *mut std::ffi::c_void,
            0, // let the browser use requestAnimationFrame
            1, // simulate_infinite_loop (never returns)
        );
    }
}

extern "C" fn em_frame_callback(arg: *mut std::ffi::c_void) {
    let game_loop = unsafe { &mut *(arg as *mut GameLoop) };

    // Sync canvas with browser viewport
    let (vw, vh, dpr) = emscripten::viewport_size_device_pixels();
    game_loop.resize_if_needed(vw, vh);
    game_loop.dpi_scale = 1.0;
    game_loop.touch_dpr = dpr as f32;

    game_loop.step();
}
