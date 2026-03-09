pub struct SpriteSheet {
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u32,
}

impl SpriteSheet {
    /// Returns the source rectangle (sx, sy, sw, sh) in pixel coordinates for a given frame.
    pub fn frame_src_rect(&self, frame_index: u32) -> (f64, f64, f64, f64) {
        let sx = (frame_index * self.frame_width) as f64;
        let sy = 0.0;
        let sw = self.frame_width as f64;
        let sh = self.frame_height as f64;
        (sx, sy, sw, sh)
    }

    /// Returns normalized UV coordinates [u_start, v_start, u_end, v_end] for a frame.
    /// Kept for compatibility with any code that uses UV coords.
    pub fn frame_uv(&self, frame_index: u32, image_width: u32) -> [f32; 4] {
        let u_start = (frame_index * self.frame_width) as f32 / image_width as f32;
        let u_end = ((frame_index + 1) * self.frame_width) as f32 / image_width as f32;
        [u_start, 0.0, u_end, 1.0]
    }
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
    fn frame_src_rect_first_frame() {
        let sheet = SpriteSheet {
            frame_width: 192,
            frame_height: 192,
            frame_count: 8,
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(0);
        assert!((sx - 0.0).abs() < f64::EPSILON);
        assert!((sy - 0.0).abs() < f64::EPSILON);
        assert!((sw - 192.0).abs() < f64::EPSILON);
        assert!((sh - 192.0).abs() < f64::EPSILON);
    }

    #[test]
    fn frame_src_rect_last_frame() {
        let sheet = SpriteSheet {
            frame_width: 192,
            frame_height: 192,
            frame_count: 8,
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(7);
        assert!((sx - 1344.0).abs() < f64::EPSILON);
        assert!((sy - 0.0).abs() < f64::EPSILON);
        assert!((sw - 192.0).abs() < f64::EPSILON);
        assert!((sh - 192.0).abs() < f64::EPSILON);
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
