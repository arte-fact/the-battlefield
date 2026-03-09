use crate::sprite::AnimationState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Dust,
    ExplosionSmall,
    ExplosionLarge,
}

impl ParticleKind {
    pub fn frame_count(self) -> u32 {
        match self {
            ParticleKind::Dust => 8,
            ParticleKind::ExplosionSmall => 8,
            ParticleKind::ExplosionLarge => 10,
        }
    }

    pub fn frame_size(self) -> u32 {
        match self {
            ParticleKind::Dust => 64,
            ParticleKind::ExplosionSmall | ParticleKind::ExplosionLarge => 192,
        }
    }

    pub fn asset_filename(self) -> &'static str {
        match self {
            ParticleKind::Dust => "Dust_01.png",
            ParticleKind::ExplosionSmall => "Explosion_01.png",
            ParticleKind::ExplosionLarge => "Explosion_02.png",
        }
    }
}

pub struct Particle {
    pub kind: ParticleKind,
    pub world_x: f32,
    pub world_y: f32,
    pub animation: AnimationState,
    pub finished: bool,
}

impl Particle {
    pub fn new(kind: ParticleKind, world_x: f32, world_y: f32) -> Self {
        Self {
            kind,
            world_x,
            world_y,
            animation: AnimationState::new(kind.frame_count(), 15.0),
            finished: false,
        }
    }

    pub fn update(&mut self, dt: f64) {
        let prev_frame = self.animation.current_frame;
        self.animation.update(dt);
        // Particle plays once, then finishes
        if self.animation.current_frame < prev_frame {
            self.finished = true;
        }
    }
}

/// An arrow projectile traveling from source to target.
pub struct Projectile {
    pub start_x: f32,
    pub start_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub speed: f32,
    pub finished: bool,
    /// Rotation angle in radians.
    pub angle: f32,
}

impl Projectile {
    pub fn new(start_x: f32, start_y: f32, target_x: f32, target_y: f32) -> Self {
        let dx = target_x - start_x;
        let dy = target_y - start_y;
        let angle = dy.atan2(dx);
        Self {
            start_x,
            start_y,
            target_x,
            target_y,
            current_x: start_x,
            current_y: start_y,
            speed: 600.0,
            finished: false,
            angle,
        }
    }

    pub fn update(&mut self, dt: f32) {
        let dx = self.target_x - self.current_x;
        let dy = self.target_y - self.current_y;
        let dist = (dx * dx + dy * dy).sqrt();
        let step = self.speed * dt;
        if step >= dist {
            self.current_x = self.target_x;
            self.current_y = self.target_y;
            self.finished = true;
        } else {
            self.current_x += dx / dist * step;
            self.current_y += dy / dist * step;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_finishes_after_one_cycle() {
        let mut p = Particle::new(ParticleKind::Dust, 0.0, 0.0);
        assert!(!p.finished);
        // Run for enough time to play all 8 frames at 15fps
        for _ in 0..20 {
            p.update(0.1);
        }
        assert!(p.finished);
    }

    #[test]
    fn projectile_reaches_target() {
        let mut proj = Projectile::new(0.0, 0.0, 100.0, 0.0);
        assert!(!proj.finished);
        // Should reach in about 100/600 = 0.167 seconds
        proj.update(0.2);
        assert!(proj.finished);
        assert!((proj.current_x - 100.0).abs() < 1e-3);
    }

    #[test]
    fn projectile_angle() {
        let proj = Projectile::new(0.0, 0.0, 100.0, 0.0);
        assert!((proj.angle).abs() < 1e-5); // pointing right = 0 radians

        let proj2 = Projectile::new(0.0, 0.0, 0.0, 100.0);
        assert!((proj2.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5); // pointing down
    }
}
