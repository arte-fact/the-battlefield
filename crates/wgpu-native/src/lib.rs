#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

static GAME_LOOP_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[wasm_bindgen(start)]
pub fn web_main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("The Battlefield -- wgpu web starting up");

    use battlefield_wgpu::game_loop::GameLoop;
    use std::sync::Arc;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::web::WindowAttributesExtWebSys;
    use winit::window::{Window, WindowAttributes, WindowId};

    struct App {
        window: Option<Arc<Window>>,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let canvas = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());

            let mut attrs = WindowAttributes::default()
                .with_title("The Battlefield")
                .with_prevent_default(true)
                .with_focusable(true);

            if let Some(canvas) = canvas {
                attrs = attrs.with_canvas(Some(canvas));
            }

            let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
            let window_clone = window.clone();
            self.window = Some(window);

            // Force touch-action after winit sets up the canvas (it may override CSS)
            if let Some(c) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("canvas"))
            {
                let _ = c
                    .dyn_ref::<web_sys::HtmlElement>()
                    .map(|el| el.style().set_property("touch-action", "none"));
            }

            wasm_bindgen_futures::spawn_local(async move {
                let mut game_loop = GameLoop::new_async(window_clone.clone()).await;
                let size = window_clone.inner_size();
                let dpr = window_clone.scale_factor();
                if size.width > 1 && size.height > 1 {
                    game_loop.resize(size.width, size.height);
                    game_loop.set_dpi(dpr);
                }
                let ptr = Box::into_raw(Box::new(game_loop));
                GAME_LOOP_PTR.store(ptr as usize, std::sync::atomic::Ordering::SeqCst);
                window_clone.request_redraw();
                log::info!("wgpu initialized, game ready");
            });
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            let ptr = GAME_LOOP_PTR.load(std::sync::atomic::Ordering::SeqCst);
            if ptr == 0 {
                return;
            }
            let game_loop = unsafe { &mut *(ptr as *mut GameLoop) };

            match &event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                    return;
                }
                WindowEvent::Resized(size) => {
                    game_loop.resize(size.width, size.height);
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(w) = &self.window {
                        game_loop.set_dpi(w.scale_factor());
                        let size = w.inner_size();
                        game_loop.resize(size.width, size.height);
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Some(w) = &self.window {
                        let size = w.inner_size();
                        let cur_w = game_loop.gpu.surface_config.width;
                        let cur_h = game_loop.gpu.surface_config.height;
                        if size.width != cur_w || size.height != cur_h {
                            game_loop.resize(size.width, size.height);
                        }
                        let dpr = w.scale_factor();
                        if (dpr - game_loop.dpi_scale).abs() > 0.01 {
                            game_loop.set_dpi(dpr);
                        }
                    }
                    game_loop.step();
                    game_loop.end_frame();
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }
                _ => {}
            }

            game_loop.handle_event(&event);
        }
    }

    let event_loop = EventLoop::new().expect("event loop");

    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(App { window: None });
}

#[wasm_bindgen]
pub fn get_ai_config() -> String {
    let ptr = GAME_LOOP_PTR.load(std::sync::atomic::Ordering::SeqCst);
    if ptr == 0 {
        return "{}".to_string();
    }
    let game_loop = unsafe { &*(ptr as *const battlefield_wgpu::game_loop::GameLoop) };
    game_loop.game.config.to_json()
}

#[wasm_bindgen]
pub fn set_ai_config(json: &str) {
    let ptr = GAME_LOOP_PTR.load(std::sync::atomic::Ordering::SeqCst);
    if ptr == 0 {
        return;
    }
    let game_loop = unsafe { &mut *(ptr as *mut battlefield_wgpu::game_loop::GameLoop) };
    if let Some(cfg) = battlefield_core::config::GameConfig::from_json(json) {
        game_loop.game.config = cfg;
    }
}
