#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

static GAME_LOOP_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static LAST_VP_W: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LAST_VP_H: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Read the browser viewport size in device pixels every frame, matching
/// SDL's `emscripten::viewport_size_device_pixels()` approach.  Winit's
/// `inner_size()` returns a cached value that can be stale during mobile
/// orientation changes.
///
/// Prefers `visualViewport` (accurate on mobile — excludes virtual keyboard,
/// tracks address-bar state) with `window.innerWidth/Height` fallback.
fn viewport_size_device_pixels() -> (u32, u32, f64) {
    use wasm_bindgen::JsValue;
    let window = web_sys::window().expect("no window");
    let dpr = window.device_pixel_ratio().max(1.0);

    // visualViewport gives the actual visible area on mobile
    let vv = js_sys::Reflect::get(&window, &JsValue::from_str("visualViewport"))
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null());
    let (css_w, css_h) = if let Some(ref vv) = vv {
        let w = js_sys::Reflect::get(vv, &JsValue::from_str("width"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let h = js_sys::Reflect::get(vv, &JsValue::from_str("height"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if w > 0.0 && h > 0.0 {
            (w, h)
        } else {
            (
                window.inner_width().unwrap().as_f64().unwrap_or(960.0),
                window.inner_height().unwrap().as_f64().unwrap_or(640.0),
            )
        }
    } else {
        (
            window.inner_width().unwrap().as_f64().unwrap_or(960.0),
            window.inner_height().unwrap().as_f64().unwrap_or(640.0),
        )
    };

    // Respect canvas CSS width when the AI panel narrows the container.
    let effective_w = window
        .document()
        .and_then(|d| d.get_element_by_id("canvas"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        .map(|el| el.offset_width() as f64)
        .filter(|&w| w > 0.0 && w < css_w)
        .unwrap_or(css_w);
    let w = (effective_w * dpr).round() as u32;
    let h = (css_h * dpr).round() as u32;
    (w.max(1), h.max(1), dpr)
}

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
                // Use JS viewport dimensions for initial size (winit's
                // inner_size may not reflect the true viewport yet).
                let (vp_w, vp_h, dpr) = viewport_size_device_pixels();
                if vp_w > 1 && vp_h > 1 {
                    LAST_VP_W.store(vp_w, std::sync::atomic::Ordering::Relaxed);
                    LAST_VP_H.store(vp_h, std::sync::atomic::Ordering::Relaxed);
                    game_loop.resize(vp_w, vp_h);
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
                    LAST_VP_W.store(size.width, std::sync::atomic::Ordering::Relaxed);
                    LAST_VP_H.store(size.height, std::sync::atomic::Ordering::Relaxed);
                    game_loop.resize(size.width, size.height);
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    let (vp_w, vp_h, dpr) = viewport_size_device_pixels();
                    LAST_VP_W.store(vp_w, std::sync::atomic::Ordering::Relaxed);
                    LAST_VP_H.store(vp_h, std::sync::atomic::Ordering::Relaxed);
                    game_loop.set_dpi(dpr);
                    game_loop.resize(vp_w, vp_h);
                }
                WindowEvent::RedrawRequested => {
                    // Poll actual viewport size from JS every frame.
                    // Compare against last raw viewport (not clamped surface
                    // config) to avoid resize thrashing on high-DPI devices.
                    let (vp_w, vp_h, dpr) = viewport_size_device_pixels();
                    let prev_w = LAST_VP_W.load(std::sync::atomic::Ordering::Relaxed);
                    let prev_h = LAST_VP_H.load(std::sync::atomic::Ordering::Relaxed);
                    if vp_w != prev_w || vp_h != prev_h {
                        LAST_VP_W.store(vp_w, std::sync::atomic::Ordering::Relaxed);
                        LAST_VP_H.store(vp_h, std::sync::atomic::Ordering::Relaxed);
                        game_loop.resize(vp_w, vp_h);
                    }
                    if (dpr - game_loop.dpi_scale).abs() > 0.01 {
                        game_loop.set_dpi(dpr);
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
