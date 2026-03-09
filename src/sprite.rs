pub struct SpriteSheet {
    pub image_data: Vec<u8>,
    pub image_width: u32,
    pub image_height: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u32,
}

impl SpriteSheet {
    pub fn frame_uv(&self, frame_index: u32) -> [f32; 4] {
        let u_start = (frame_index * self.frame_width) as f32 / self.image_width as f32;
        let u_end = ((frame_index + 1) * self.frame_width) as f32 / self.image_width as f32;
        [u_start, 0.0, u_end, 1.0]
    }
}

#[cfg(target_arch = "wasm32")]
impl SpriteSheet {
    pub async fn from_url(
        url: &str,
        frame_width: u32,
        frame_height: u32,
        frame_count: u32,
    ) -> Result<Self, wasm_bindgen::JsValue> {
        use wasm_bindgen::JsValue;

        let image_data = fetch_image_bytes(url).await?;
        let img = image::load_from_memory_with_format(&image_data, image::ImageFormat::Png)
            .map_err(|e| JsValue::from_str(&format!("Failed to decode PNG: {e}")))?;
        let rgba = img.to_rgba8();
        let image_width = rgba.width();
        let image_height = rgba.height();

        log::info!(
            "Loaded sprite sheet: {url} ({}x{}, {frame_count} frames of {frame_width}x{frame_height})",
            image_width, image_height
        );

        Ok(Self {
            image_data: rgba.into_raw(),
            image_width,
            image_height,
            frame_width,
            frame_height,
            frame_count,
        })
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_image_bytes(url: &str) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or("no window")?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await?;
    let resp: web_sys::Response = resp_value.dyn_into()?;
    if !resp.ok() {
        return Err(wasm_bindgen::JsValue::from_str(&format!(
            "Failed to fetch {url}: {}",
            resp.status()
        )));
    }
    let array_buffer = wasm_bindgen_futures::JsFuture::from(resp.array_buffer()?).await?;
    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    Ok(uint8_array.to_vec())
}

pub struct AnimationState {
    pub current_frame: u32,
    pub frame_timer: f64,
    pub frame_duration: f64,
    pub frame_count: u32,
}

impl AnimationState {
    pub fn new(frame_count: u32, fps: f64) -> Self {
        Self {
            current_frame: 0,
            frame_timer: 0.0,
            frame_duration: 1.0 / fps,
            frame_count,
        }
    }

    pub fn update(&mut self, dt: f64) {
        self.frame_timer += dt;
        while self.frame_timer >= self.frame_duration {
            self.frame_timer -= self.frame_duration;
            self.current_frame = (self.current_frame + 1) % self.frame_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_uv_first_frame() {
        let sheet = SpriteSheet {
            image_data: vec![],
            image_width: 1536,
            image_height: 192,
            frame_width: 192,
            frame_height: 192,
            frame_count: 8,
        };
        let uv = sheet.frame_uv(0);
        assert!((uv[0] - 0.0).abs() < f32::EPSILON);
        assert!((uv[2] - 0.125).abs() < f32::EPSILON);
    }

    #[test]
    fn frame_uv_last_frame() {
        let sheet = SpriteSheet {
            image_data: vec![],
            image_width: 1536,
            image_height: 192,
            frame_width: 192,
            frame_height: 192,
            frame_count: 8,
        };
        let uv = sheet.frame_uv(7);
        assert!((uv[0] - 0.875).abs() < f32::EPSILON);
        assert!((uv[2] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn animation_state_advances_frame() {
        let mut anim = AnimationState::new(8, 10.0);
        assert_eq!(anim.current_frame, 0);
        anim.update(0.1);
        assert_eq!(anim.current_frame, 1);
        anim.update(0.1);
        assert_eq!(anim.current_frame, 2);
    }

    #[test]
    fn animation_state_wraps_around() {
        let mut anim = AnimationState::new(4, 10.0);
        for _ in 0..4 {
            anim.update(0.1);
        }
        assert_eq!(anim.current_frame, 0);
    }
}
