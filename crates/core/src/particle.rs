use crate::sprite::AnimationState;
use crate::unit::Faction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleKind {
    Dust,
    ExplosionLarge,
}

impl ParticleKind {
    pub fn frame_count(self) -> u32 {
        match self {
            ParticleKind::Dust => 8,
            ParticleKind::ExplosionLarge => 10,
        }
    }

    pub fn frame_size(self) -> u32 {
        match self {
            ParticleKind::Dust => 64,
            ParticleKind::ExplosionLarge => 192,
        }
    }

    pub fn asset_filename(self) -> &'static str {
        match self {
            ParticleKind::Dust => "Dust_01.png",
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

/// Arrow speed in pixels per second.
const ARROW_SPEED: f32 = 600.0;
/// Minimum arc height in pixels.
const ARC_BASE: f32 = 30.0;
/// Extra arc height per pixel of horizontal distance.
const ARC_DISTANCE_FACTOR: f32 = 0.25;

/// An arrow projectile following a ballistic (parabolic) arc from source to target.
pub struct Projectile {
    pub start_x: f32,
    pub start_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub finished: bool,
    /// Rotation angle in radians (updated each frame from arc tangent).
    pub angle: f32,
    /// Flight progress 0.0 → 1.0.
    progress: f32,
    /// Total flight time in seconds.
    duration: f32,
    /// Peak height of parabola in pixels.
    arc_height: f32,
    /// Damage to apply on impact (0 = cosmetic miss arrow).
    pub damage: i32,
    /// Faction of the archer who fired (enemies of this faction can be hit).
    pub faction: Faction,
}

impl Projectile {
    pub fn new(
        start_x: f32,
        start_y: f32,
        target_x: f32,
        target_y: f32,
        damage: i32,
        faction: Faction,
    ) -> Self {
        let dx = target_x - start_x;
        let dy = target_y - start_y;
        let distance = (dx * dx + dy * dy).sqrt();
        let duration = (distance / ARROW_SPEED).max(0.1);
        let arc_height = ARC_BASE + distance * ARC_DISTANCE_FACTOR;

        // Initial angle: tangent of the arc at t=0
        // dz/dt at t=0 = arc_height * 4 (upward), screen-space up is -y
        let initial_dz = arc_height * 4.0;
        let angle = (dy - initial_dz).atan2(dx);

        Self {
            start_x,
            start_y,
            target_x,
            target_y,
            current_x: start_x,
            current_y: start_y,
            finished: false,
            angle,
            progress: 0.0,
            duration,
            arc_height,
            damage,
            faction,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.progress += dt / self.duration;
        if self.progress >= 1.0 {
            self.progress = 1.0;
            self.current_x = self.target_x;
            self.current_y = self.target_y;
            self.finished = true;
            // Final angle: arrow diving down into target
            let dx = self.target_x - self.start_x;
            let dy = self.target_y - self.start_y;
            let dz = self.arc_height * 4.0; // |slope| at t=1
            self.angle = (dy + dz).atan2(dx);
            return;
        }

        let t = self.progress;

        // Ground-plane position: linear lerp
        let ground_x = self.start_x + (self.target_x - self.start_x) * t;
        let ground_y = self.start_y + (self.target_y - self.start_y) * t;

        // Parabolic height: z = arc_height * 4 * t * (1 - t), peaks at t=0.5
        let z = self.arc_height * 4.0 * t * (1.0 - t);

        // Screen position: offset Y upward by z (screen Y goes down)
        self.current_x = ground_x;
        self.current_y = ground_y - z;

        // Angle from tangent: d(z)/dt = arc_height * 4 * (1 - 2t)
        let dx = self.target_x - self.start_x;
        let dy = self.target_y - self.start_y;
        let dz_dt = self.arc_height * 4.0 * (1.0 - 2.0 * t);
        // Screen-space tangent: (dx, dy - dz_dt) per unit progress
        self.angle = (dy - dz_dt).atan2(dx);
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
        let mut proj = Projectile::new(0.0, 0.0, 100.0, 0.0, 2, Faction::Blue);
        assert!(!proj.finished);
        // duration = 100/600 ≈ 0.167s, run in small steps
        for _ in 0..20 {
            proj.update(0.016);
        }
        assert!(proj.finished);
        assert!((proj.current_x - 100.0).abs() < 1e-3);
        // At landing, current_y should be back at ground level (target_y = 0)
        assert!((proj.current_y - 0.0).abs() < 1e-3);
    }

    #[test]
    fn projectile_arc_rises_at_midpoint() {
        let mut proj = Projectile::new(0.0, 0.0, 600.0, 0.0, 2, Faction::Blue);
        // duration = 600/600 = 1.0s, advance to midpoint
        proj.update(0.5);
        // At midpoint, arrow should be above the ground plane (negative Y in screen space)
        assert!(
            proj.current_y < -10.0,
            "Arrow should arc above ground at midpoint, got y={}",
            proj.current_y
        );
        // X should be roughly at midpoint
        assert!((proj.current_x - 300.0).abs() < 5.0);
    }

    #[test]
    fn projectile_angle_tilts_up_then_down() {
        let mut proj = Projectile::new(0.0, 0.0, 600.0, 0.0, 2, Faction::Blue);
        // At launch, angle should be negative (tilted upward in screen space)
        assert!(
            proj.angle < 0.0,
            "Arrow should tilt upward at launch, got angle={}",
            proj.angle
        );

        // Advance past midpoint
        proj.update(0.75);
        // After midpoint, angle should be positive (tilted downward)
        assert!(
            proj.angle > 0.0,
            "Arrow should tilt downward after midpoint, got angle={}",
            proj.angle
        );
    }
}
