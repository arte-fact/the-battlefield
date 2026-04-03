#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::sync::Arc;

    use battlefield_wgpu::game_loop::{GameLoop, WINDOW_H, WINDOW_W};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::window::{Window, WindowAttributes, WindowId};

    struct App {
        window: Option<Arc<Window>>,
        game_loop: Option<GameLoop>,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_none() {
                let attrs = WindowAttributes::default()
                    .with_title("The Battlefield")
                    .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_W, WINDOW_H));
                let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
                self.game_loop = Some(GameLoop::new(window.clone()));
                window.request_redraw();
                self.window = Some(window);
                log::info!("Window created, wgpu initialized");
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            let Some(game_loop) = self.game_loop.as_mut() else {
                return;
            };

            match &event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                    return;
                }
                WindowEvent::Resized(size) => {
                    game_loop.resize(size.width, size.height);
                }
                WindowEvent::RedrawRequested => {
                    if !game_loop.step() {
                        event_loop.exit();
                        return;
                    }
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

    env_logger::init();
    log::info!("The Battlefield -- wgpu native starting up");

    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App {
        window: None,
        game_loop: None,
    };
    event_loop.run_app(&mut app).expect("run app");
    log::info!("Shutting down");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // Web entry point is in lib.rs via wasm_bindgen(start)
}
