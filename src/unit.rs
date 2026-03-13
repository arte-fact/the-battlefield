use crate::grid;
use crate::sprite::AnimationState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Faction {
    Blue,
    Red,
    Purple,
    Yellow,
    Black,
}

impl Faction {
    pub fn asset_folder(self) -> &'static str {
        match self {
            Faction::Blue => "Blue Units",
            Faction::Red => "Red Units",
            Faction::Purple => "Purple Units",
            Faction::Yellow => "Yellow Units",
            Faction::Black => "Black Units",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Warrior,
    Archer,
    Lancer,
    Pawn,
    Monk,
}

impl UnitKind {
    pub fn base_stats(self) -> UnitStats {
        match self {
            UnitKind::Warrior => UnitStats {
                max_hp: 10,
                atk: 3,
                def: 3,
                mov: 3,
                range: 1,
            },
            UnitKind::Archer => UnitStats {
                max_hp: 6,
                atk: 2,
                def: 1,
                mov: 3,
                range: 5,
            },
            UnitKind::Lancer => UnitStats {
                max_hp: 10,
                atk: 4,
                def: 1,
                mov: 5,
                range: 1,
            },
            UnitKind::Pawn => UnitStats {
                max_hp: 7,
                atk: 2,
                def: 1,
                mov: 4,
                range: 1,
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
            UnitKind::Pawn => 8,
            UnitKind::Monk => 6,
        }
    }

    pub fn run_frames(self) -> u32 {
        match self {
            UnitKind::Warrior => 6,
            UnitKind::Archer => 4,
            UnitKind::Lancer => 6,
            UnitKind::Pawn => 6,
            UnitKind::Monk => 4,
        }
    }

    pub fn attack_frames(self) -> u32 {
        match self {
            UnitKind::Warrior => 4,
            UnitKind::Archer => 8, // shoot
            UnitKind::Lancer => 3, // directional attack
            UnitKind::Pawn => 6,   // axe interact
            UnitKind::Monk => 11,  // heal
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

/// Duration of the death fade-out animation in seconds.
pub const DEATH_FADE_DURATION: f32 = 0.3;

pub type UnitId = u32;

pub struct Unit {
    pub id: UnitId,
    pub kind: UnitKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
    pub hp: i32,
    pub stats: UnitStats,
    pub facing: Facing,
    pub current_anim: UnitAnim,
    pub animation: AnimationState,
    pub is_player: bool,
    pub alive: bool,
    /// Movement points remaining this turn.
    pub movement_left: u32,
    /// Whether this unit has attacked this turn.
    pub has_attacked: bool,
    /// Whether this unit has moved this turn.
    pub has_moved: bool,
    /// Remaining seconds of death fade-out (0.0 = not dying or fully faded).
    pub death_fade: f32,
    /// World-space visual X position (for animation interpolation).
    pub visual_x: f32,
    /// World-space visual Y position (for animation interpolation).
    pub visual_y: f32,
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
        let (vx, vy) = grid::grid_to_world(grid_x, grid_y);
        Self {
            id,
            kind,
            faction,
            grid_x,
            grid_y,
            hp: stats.max_hp,
            stats,
            facing: Facing::Right,
            current_anim: UnitAnim::Idle,
            animation: AnimationState::new(kind.idle_frames(), 10.0),
            is_player,
            alive: true,
            movement_left: stats.mov,
            has_attacked: false,
            has_moved: false,
            death_fade: 0.0,
            visual_x: vx,
            visual_y: vy,
        }
    }

    pub fn take_damage(&mut self, damage: i32) {
        self.hp -= damage;
        if self.hp <= 0 {
            self.hp = 0;
            self.alive = false;
            self.death_fade = DEATH_FADE_DURATION;
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

    pub fn reset_turn(&mut self) {
        self.movement_left = self.stats.mov;
        self.has_attacked = false;
        self.has_moved = false;
        self.set_anim(UnitAnim::Idle);
    }

    /// Distance in grid cells (Chebyshev distance for attack range).
    pub fn distance_to(&self, x: u32, y: u32) -> u32 {
        let dx = (self.grid_x as i32 - x as i32).unsigned_abs();
        let dy = (self.grid_y as i32 - y as i32).unsigned_abs();
        dx.max(dy)
    }

    /// Manhattan distance for movement pathfinding.
    pub fn manhattan_distance_to(&self, x: u32, y: u32) -> u32 {
        let dx = (self.grid_x as i32 - x as i32).unsigned_abs();
        let dy = (self.grid_y as i32 - y as i32).unsigned_abs();
        dx + dy
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
        assert_eq!(stats.mov, 3);
        assert_eq!(stats.range, 1);
    }

    #[test]
    fn unit_creation() {
        let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 10, false);
        assert_eq!(unit.hp, 10);
        assert_eq!(unit.grid_x, 5);
        assert_eq!(unit.grid_y, 10);
        assert!(unit.alive);
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
    fn distance_calculation() {
        let unit = Unit::new(1, UnitKind::Archer, Faction::Blue, 5, 5, false);
        assert_eq!(unit.distance_to(5, 5), 0);
        assert_eq!(unit.distance_to(6, 5), 1);
        assert_eq!(unit.distance_to(6, 6), 1); // Chebyshev
        assert_eq!(unit.distance_to(10, 5), 5);
    }

    #[test]
    fn reset_turn() {
        let mut unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 0, 0, false);
        unit.movement_left = 0;
        unit.has_attacked = true;
        unit.reset_turn();
        assert_eq!(unit.movement_left, 3);
        assert!(!unit.has_attacked);
    }

    #[test]
    fn lancer_frame_size() {
        assert_eq!(UnitKind::Lancer.frame_size(), 320);
        assert_eq!(UnitKind::Warrior.frame_size(), 192);
    }

    #[test]
    fn unit_visual_position_initialized() {
        use crate::grid;
        let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 10, false);
        let (expected_x, expected_y) = grid::grid_to_world(5, 10);
        assert!((unit.visual_x - expected_x).abs() < f32::EPSILON);
        assert!((unit.visual_y - expected_y).abs() < f32::EPSILON);
    }

    #[test]
    fn death_fade_starts_on_kill() {
        let mut unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 0, 0, false);
        assert!((unit.death_fade).abs() < f32::EPSILON);
        unit.take_damage(100);
        assert!(!unit.alive);
        assert!((unit.death_fade - DEATH_FADE_DURATION).abs() < f32::EPSILON);
    }
}
