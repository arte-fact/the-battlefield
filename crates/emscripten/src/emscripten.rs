extern "C" {
    pub fn emscripten_set_main_loop_arg(
        func: extern "C" fn(*mut std::ffi::c_void),
        arg: *mut std::ffi::c_void,
        fps: std::ffi::c_int,
        simulate_infinite_loop: std::ffi::c_int,
    );
    #[allow(dead_code)]
    pub fn emscripten_cancel_main_loop();
    /// Execute a JS expression and return the integer result.
    pub fn emscripten_run_script_int(script: *const std::ffi::c_char) -> std::ffi::c_int;
}

/// Get the browser device pixel ratio.
pub fn device_pixel_ratio() -> f64 {
    unsafe {
        // Returns DPR * 100 to avoid float truncation, then divide
        let dpr100 =
            emscripten_run_script_int(c"Math.round((window.devicePixelRatio||1)*100)".as_ptr());
        (dpr100 as f64 / 100.0).max(1.0)
    }
}

/// Get the browser viewport size in actual device pixels (CSS pixels * DPR).
pub fn viewport_size_device_pixels() -> (u32, u32, f64) {
    let dpr = device_pixel_ratio();
    unsafe {
        let css_w = emscripten_run_script_int(c"window.innerWidth".as_ptr());
        let css_h = emscripten_run_script_int(c"window.innerHeight".as_ptr());
        let w = (css_w as f64 * dpr).round() as u32;
        let h = (css_h as f64 * dpr).round() as u32;
        (w.max(320), h.max(240), dpr)
    }
}
