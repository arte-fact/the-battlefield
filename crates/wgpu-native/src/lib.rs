#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn web_main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("The Battlefield -- wgpu web starting up");

    use battlefield_wgpu::game_loop::{GameLoop, WINDOW_H, WINDOW_W};
    use std::sync::Arc;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::web::WindowAttributesExtWebSys;
    use winit::window::{Window, WindowAttributes, WindowId};

    struct App {
        window: Option<Arc<Window>>,
    }

    static GAME_LOOP_PTR: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

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
                .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_W, WINDOW_H));

            if let Some(canvas) = canvas {
                attrs = attrs.with_canvas(Some(canvas));
            }

            let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
            let window_clone = window.clone();
            self.window = Some(window);

            wasm_bindgen_futures::spawn_local(async move {
                let game_loop = GameLoop::new_async(window_clone.clone()).await;
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
                WindowEvent::RedrawRequested => {
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
