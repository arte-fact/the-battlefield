use crate::grid::{self, TILE_SIZE};
use crate::sprite::AnimationState;

/// Collision circle radius for all units.
pub const UNIT_RADIUS: f32 = 28.0;

/// Melee attack reach in world pixels.
pub const MELEE_RANGE: f32 = TILE_SIZE * 1.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Faction {
    Blue,
    Red,
}

impl Faction {
    pub fn asset_folder(self) -> &'static str {
        match self {
            Faction::Blue => "Blue Units",
            Faction::Red => "Red Units",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Warrior,
    Archer,
    Lancer,
    Monk,
}

impl UnitKind {
    pub fn base_stats(self) -> UnitStats {
        match self {
            UnitKind::Warrior => UnitStats {
                max_hp: 10,
                atk: 3,
                def: 3,
                mov: 5,
                range: 1,
            },
            UnitKind::Archer => UnitStats {
                max_hp: 6,
                atk: 2,
                def: 1,
                mov: 4,
                range: 7,
            },
            UnitKind::Lancer => UnitStats {
                max_hp: 10,
                atk: 4,
                def: 1,
                mov: 4,
                range: 2,
            },
            UnitKind::Monk => UnitStats {
                max_hp: 5,
                atk: 1,
                def: 1,
                mov: 3,
                range: 2,
            },
        }
    }

    pub fn frame_size(self) -> u32 {
        match self {
            UnitKind::Lancer => 320,
            _ => 192,
        }
    }

    pub fn idle_frames(self) -> u32 {
        match self {
            UnitKind::Warrior => 8,
            UnitKind::Archer => 6,
            UnitKind::Lancer => 12,
            UnitKind::Monk => 6,
        }
    }

    pub fn run_frames(self) -> u32 {
        match self {
            UnitKind::Warrior => 6,
            UnitKind::Archer => 4,
            UnitKind::Lancer => 6,
            UnitKind::Monk => 4,
        }
    }

    pub fn attack_frames(self) -> u32 {
        match self {
            UnitKind::Warrior => 4,
            UnitKind::Archer => 8, // shoot
            UnitKind::Lancer => 3, // directional attack
            UnitKind::Monk => 11,  // heal
        }
    }

    /// Base attack cooldown in seconds.
    pub fn base_attack_cooldown(self) -> f32 {
        match self {
            UnitKind::Warrior => 0.60,
            UnitKind::Lancer => 0.50,
            UnitKind::Archer => 0.80,
            UnitKind::Monk => 0.70,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitStats {
    pub max_hp: i32,
    pub atk: i32,
    pub def: i32,
    pub mov: u32,
    pub range: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitAnim {
    Idle,
    Run,
    Attack,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderKind {
    Hold { target_x: f32, target_y: f32 },
    Go { target_x: f32, target_y: f32 },
    Retreat { target_x: f32, target_y: f32 },
    Follow,
}

/// Duration of the death fade-out animation in seconds.
pub const DEATH_FADE_DURATION: f32 = 0.3;

pub type UnitId = u32;

pub struct Unit {
    pub id: UnitId,
    pub kind: UnitKind,
    pub faction: Faction,
    /// World-space X position (canonical + rendered).
    pub x: f32,
    /// World-space Y position (canonical + rendered).
    pub y: f32,
    pub hp: i32,
    pub stats: UnitStats,
    pub facing: Facing,
    pub current_anim: UnitAnim,
    pub animation: AnimationState,
    pub is_player: bool,
    pub alive: bool,
    /// Remaining seconds of death fade-out (0.0 = not dying or fully faded).
    pub death_fade: f32,
    /// Real-time attack cooldown (0.0 = ready to attack).
    pub attack_cooldown: f32,
    /// AI waypoints in world-space (for pathfinding).
    pub ai_waypoints: Vec<(f32, f32)>,
    /// Current index into ai_waypoints.
    pub ai_waypoint_idx: usize,
    /// Cooldown before re-running A* (avoids pathing every frame).
    pub ai_path_cooldown: f32,
    /// Remaining seconds of hit flash blink effect (0.0 = not flashing).
    pub hit_flash: f32,
    /// Active player order (None = default AI behavior).
    pub order: Option<OrderKind>,
    /// Seconds remaining for order flash indicator.
    pub order_flash: f32,
    /// Cached nearest enemy result (refreshed periodically, not every frame).
    pub cached_enemy: Option<(f32, f32, UnitId, f32)>,
    /// Cooldown before refreshing cached_enemy (avoids LOS raycasts every frame).
    pub enemy_scan_cooldown: f32,
}

impl Unit {
    pub fn new(
        id: UnitId,
        kind: UnitKind,
        faction: Faction,
        grid_x: u32,
        grid_y: u32,
        is_player: bool,
    ) -> Self {
        let stats = kind.base_stats();
        let (wx, wy) = grid::grid_to_world(grid_x, grid_y);
        Self {
            id,
            kind,
            faction,
            x: wx,
            y: wy,
            hp: stats.max_hp,
            stats,
            facing: Facing::Right,
            current_anim: UnitAnim::Idle,
            animation: AnimationState::new(kind.idle_frames(), 10.0),
            is_player,
            alive: true,
            death_fade: 0.0,
            attack_cooldown: 0.0,
            ai_waypoints: Vec::new(),
            ai_waypoint_idx: 0,
            ai_path_cooldown: 0.0,
            hit_flash: 0.0,
            order: None,
            order_flash: 0.0,
            cached_enemy: None,
            enemy_scan_cooldown: 0.0,
        }
    }

    pub fn take_damage(&mut self, damage: i32) {
        self.hp -= damage;
        self.hit_flash = 0.15;
        if self.hp <= 0 {
            self.hp = 0;
            self.alive = false;
            self.death_fade = DEATH_FADE_DURATION;
            self.order = None;
            self.set_anim(UnitAnim::Idle);
        }
    }

    pub fn set_anim(&mut self, anim: UnitAnim) {
        if self.current_anim != anim {
            self.current_anim = anim;
            let (frames, fps) = match anim {
                UnitAnim::Idle => (self.kind.idle_frames(), 10.0),
                UnitAnim::Run => (self.kind.run_frames(), 12.0),
                UnitAnim::Attack => (self.kind.attack_frames(), 12.0),
            };
            self.animation = AnimationState::new(frames, fps);
        }
    }

    /// Whether this unit is ready to act/attack (alive and attack cooldown expired).
    pub fn can_act(&self) -> bool {
        self.alive && self.attack_cooldown <= 0.0
    }

    /// Tick attack cooldown by dt.
    pub fn tick_cooldowns(&mut self, dt: f32) {
        self.attack_cooldown = (self.attack_cooldown - dt).max(0.0);
        self.hit_flash = (self.hit_flash - dt).max(0.0);
        self.order_flash = (self.order_flash - dt).max(0.0);
    }

    /// Start attack cooldown.
    pub fn start_attack_cooldown(&mut self) {
        self.attack_cooldown = self.kind.base_attack_cooldown();
    }

    /// Grid cell this unit is currently standing on.
    pub fn grid_cell(&self) -> (u32, u32) {
        let (gx, gy) = grid::world_to_grid(self.x, self.y);
        (gx.max(0) as u32, gy.max(0) as u32)
    }

    /// Movement speed in pixels/sec based on mov stat.
    pub fn move_speed(&self) -> f32 {
        TILE_SIZE * self.stats.mov as f32 / 0.90
    }

    /// Euclidean distance to a world position.
    pub fn distance_to_pos(&self, ox: f32, oy: f32) -> f32 {
        let dx = self.x - ox;
        let dy = self.y - oy;
        (dx * dx + dy * dy).sqrt()
    }

    /// Euclidean distance to another unit.
    pub fn distance_to_unit(&self, other: &Unit) -> f32 {
        self.distance_to_pos(other.x, other.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warrior_stats() {
        let stats = UnitKind::Warrior.base_stats();
        assert_eq!(stats.max_hp, 10);
        assert_eq!(stats.atk, 3);
        assert_eq!(stats.def, 3);
        assert_eq!(stats.mov, 5);
        assert_eq!(stats.range, 1);
    }

    #[test]
    fn unit_creation() {
        let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 10, false);
        assert_eq!(unit.hp, 10);
        assert_eq!(unit.grid_cell(), (5, 10));
        assert!(unit.alive);
        let (wx, wy) = grid::grid_to_world(5, 10);
        assert!((unit.x - wx).abs() < f32::EPSILON);
        assert!((unit.y - wy).abs() < f32::EPSILON);
    }

    #[test]
    fn unit_take_damage() {
        let mut unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 0, 0, false);
        unit.take_damage(3);
        assert_eq!(unit.hp, 7);
        assert!(unit.alive);
        unit.take_damage(10);
        assert_eq!(unit.hp, 0);
        assert!(!unit.alive);
    }

    #[test]
    fn euclidean_distance() {
        let a = Unit::new(1, UnitKind::Archer, Faction::Blue, 5, 5, false);
        let b = Unit::new(2, UnitKind::Warrior, Faction::Red, 5, 5, false);
        assert!(a.distance_to_unit(&b) < 1.0); // same cell
        let c = Unit::new(3, UnitKind::Warrior, Faction::Red, 6, 5, false);
        let dist = a.distance_to_unit(&c);
        assert!((dist - TILE_SIZE).abs() < 1.0); // one tile apart
    }

    #[test]
    fn lancer_frame_size() {
        assert_eq!(UnitKind::Lancer.frame_size(), 320);
        assert_eq!(UnitKind::Warrior.frame_size(), 192);
    }

    #[test]
    fn death_fade_starts_on_kill() {
        let mut unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 0, 0, false);
        assert!((unit.death_fade).abs() < f32::EPSILON);
        unit.take_damage(100);
        assert!(!unit.alive);
        assert!((unit.death_fade - DEATH_FADE_DURATION).abs() < f32::EPSILON);
    }

    #[test]
    fn move_speed_values() {
        // Warrior (mov 5): 64 * 5 / 0.90 ≈ 355.6
        let warrior = Unit::new(1, UnitKind::Warrior, Faction::Blue, 0, 0, false);
        assert!((warrior.move_speed() - 355.56).abs() < 1.0);
        // Lancer (mov 4): 64 * 4 / 0.90 ≈ 284.4
        let lancer = Unit::new(2, UnitKind::Lancer, Faction::Blue, 0, 0, false);
        assert!((lancer.move_speed() - 284.44).abs() < 1.0);
    }

    #[test]
    fn grid_cell_from_world_position() {
        let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 10, 15, false);
        assert_eq!(unit.grid_cell(), (10, 15));
    }
}
