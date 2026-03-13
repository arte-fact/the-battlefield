use std::collections::{HashSet, VecDeque};

use crate::grid;
use crate::particle::ParticleKind;
use crate::unit::{Unit, UnitAnim, UnitId};

// Timing constants
const MOVE_DURATION_SINGLE: f32 = 0.25;
const MOVE_DURATION_AUTO: f32 = 0.15;
const AI_STAGGER_DELAY: f32 = 0.05;
const MELEE_ATTACK_DURATION: f32 = 0.35;
const RANGED_ATTACK_DURATION: f32 = 0.5;

/// Events recorded during game logic execution for animation playback.
#[derive(Clone, Debug)]
pub enum TurnEvent {
    Move {
        unit_id: UnitId,
        from: (u32, u32),
        to: (u32, u32),
    },
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
    },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
        target_pos: (u32, u32),
        missed: bool,
    },
}

/// Easing: deceleration curve.
pub fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

pub struct MoveAnim {
    pub unit_id: UnitId,
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub delay: f32,
    pub duration: f32,
    pub started: bool,
}

pub enum AnimPhase {
    ParallelMoves {
        moves: Vec<MoveAnim>,
        total_duration: f32,
    },
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        #[allow(dead_code)]
        damage: i32,
        killed: bool,
        duration: f32,
        particle_spawned: bool,
    },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        target_pos: (f32, f32),
        missed: bool,
        #[allow(dead_code)]
        damage: i32,
        killed: bool,
        duration: f32,
        projectile_spawned: bool,
    },
}

pub struct AnimOutput {
    pub particles: Vec<(ParticleKind, f32, f32)>,
    pub projectiles: Vec<(f32, f32, f32, f32)>,
}

impl AnimOutput {
    fn new() -> Self {
        Self {
            particles: Vec::new(),
            projectiles: Vec::new(),
        }
    }
}

pub struct TurnAnimator {
    phases: VecDeque<AnimPhase>,
    phase_elapsed: f32,
    visual_alive: HashSet<UnitId>,
}

impl Default for TurnAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnAnimator {
    pub fn new() -> Self {
        Self {
            phases: VecDeque::new(),
            phase_elapsed: 0.0,
            visual_alive: HashSet::new(),
        }
    }

    pub fn enqueue(&mut self, events: Vec<TurnEvent>, is_auto_move: bool) {
        let move_duration = if is_auto_move {
            MOVE_DURATION_AUTO
        } else {
            MOVE_DURATION_SINGLE
        };

        let mut moves: Vec<MoveAnim> = Vec::new();
        let mut melee_attacks: Vec<TurnEvent> = Vec::new();
        let mut ranged_attacks: Vec<TurnEvent> = Vec::new();

        for event in events {
            match event {
                TurnEvent::Move { unit_id, from, to } => {
                    let delay = if moves.is_empty() {
                        0.0
                    } else {
                        moves.len() as f32 * AI_STAGGER_DELAY
                    };
                    let (fx, fy) = grid::grid_to_world(from.0, from.1);
                    let (tx, ty) = grid::grid_to_world(to.0, to.1);
                    moves.push(MoveAnim {
                        unit_id,
                        from: (fx, fy),
                        to: (tx, ty),
                        delay,
                        duration: move_duration,
                        started: false,
                    });
                }
                TurnEvent::MeleeAttack { .. } => melee_attacks.push(event),
                TurnEvent::RangedAttack { .. } => ranged_attacks.push(event),
            }
        }

        if !moves.is_empty() {
            let total_duration = moves
                .iter()
                .map(|m| m.delay + m.duration)
                .fold(0.0_f32, f32::max);
            self.phases.push_back(AnimPhase::ParallelMoves {
                moves,
                total_duration,
            });
        }

        for event in melee_attacks {
            if let TurnEvent::MeleeAttack {
                attacker_id,
                defender_id,
                damage,
                killed,
            } = event
            {
                self.phases.push_back(AnimPhase::MeleeAttack {
                    attacker_id,
                    defender_id,
                    damage,
                    killed,
                    duration: MELEE_ATTACK_DURATION,
                    particle_spawned: false,
                });
            }
        }

        for event in ranged_attacks {
            if let TurnEvent::RangedAttack {
                attacker_id,
                defender_id,
                damage,
                killed,
                target_pos,
                missed,
            } = event
            {
                let (tx, ty) = grid::grid_to_world(target_pos.0, target_pos.1);
                self.phases.push_back(AnimPhase::RangedAttack {
                    attacker_id,
                    defender_id,
                    target_pos: (tx, ty),
                    missed,
                    damage,
                    killed,
                    duration: RANGED_ATTACK_DURATION,
                    projectile_spawned: false,
                });
            }
        }

        if !self.phases.is_empty() {
            self.phase_elapsed = 0.0;
        }
    }

    /// Initialize the set of visually alive units before enqueueing events.
    pub fn init_visual_alive(&mut self, alive_ids: impl Iterator<Item = UnitId>) {
        self.visual_alive.clear();
        self.visual_alive.extend(alive_ids);
    }

    pub fn update(&mut self, dt: f32, units: &mut [Unit]) -> AnimOutput {
        let mut output = AnimOutput::new();

        if self.phases.is_empty() {
            return output;
        }

        self.phase_elapsed += dt;

        let phase_done = match self.phases.front_mut() {
            Some(AnimPhase::ParallelMoves {
                moves,
                total_duration,
            }) => {
                for mv in moves.iter_mut() {
                    let raw_t = if mv.duration > 0.0 {
                        ((self.phase_elapsed - mv.delay) / mv.duration).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let t = ease_out_quad(raw_t);

                    // Spawn dust when move starts
                    if !mv.started && self.phase_elapsed >= mv.delay {
                        mv.started = true;
                        output
                            .particles
                            .push((ParticleKind::Dust, mv.from.0, mv.from.1));
                    }

                    // Lerp visual position
                    if let Some(unit) = units.iter_mut().find(|u| u.id == mv.unit_id) {
                        unit.visual_x = mv.from.0 + (mv.to.0 - mv.from.0) * t;
                        unit.visual_y = mv.from.1 + (mv.to.1 - mv.from.1) * t;

                        if raw_t > 0.0 && raw_t < 1.0 {
                            unit.set_anim(UnitAnim::Run);
                        } else if raw_t >= 1.0 {
                            unit.set_anim(UnitAnim::Idle);
                        }
                    }
                }
                self.phase_elapsed >= *total_duration
            }
            Some(AnimPhase::MeleeAttack {
                attacker_id,
                defender_id,
                killed,
                duration,
                particle_spawned,
                ..
            }) => {
                let attacker_id = *attacker_id;
                let defender_id = *defender_id;
                let killed = *killed;
                let duration = *duration;

                if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                    unit.set_anim(UnitAnim::Attack);
                }

                if !*particle_spawned && self.phase_elapsed >= duration * 0.6 {
                    *particle_spawned = true;
                    if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                        output.particles.push((
                            ParticleKind::ExplosionSmall,
                            defender.visual_x,
                            defender.visual_y,
                        ));
                    }
                }

                let done = self.phase_elapsed >= duration;
                if done {
                    if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                        unit.set_anim(UnitAnim::Idle);
                    }
                    if killed {
                        self.visual_alive.remove(&defender_id);
                        if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                            output.particles.push((
                                ParticleKind::ExplosionLarge,
                                defender.visual_x,
                                defender.visual_y,
                            ));
                        }
                    }
                }
                done
            }
            Some(AnimPhase::RangedAttack {
                attacker_id,
                defender_id,
                target_pos,
                missed,
                killed,
                duration,
                projectile_spawned,
                ..
            }) => {
                let attacker_id = *attacker_id;
                let defender_id = *defender_id;
                let target_pos = *target_pos;
                let missed = *missed;
                let killed = *killed;
                let duration = *duration;

                if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                    unit.set_anim(UnitAnim::Attack);
                }

                if !*projectile_spawned {
                    *projectile_spawned = true;
                    if let Some(attacker) = units.iter().find(|u| u.id == attacker_id) {
                        output.projectiles.push((
                            attacker.visual_x,
                            attacker.visual_y,
                            target_pos.0,
                            target_pos.1,
                        ));
                    }
                }

                let done = self.phase_elapsed >= duration;
                if done {
                    if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                        unit.set_anim(UnitAnim::Idle);
                    }
                    if missed {
                        output
                            .particles
                            .push((ParticleKind::Dust, target_pos.0, target_pos.1));
                    } else if killed {
                        self.visual_alive.remove(&defender_id);
                        if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                            output.particles.push((
                                ParticleKind::ExplosionLarge,
                                defender.visual_x,
                                defender.visual_y,
                            ));
                        }
                    }
                }
                done
            }
            None => false,
        };

        if phase_done {
            self.phases.pop_front();
            self.phase_elapsed = 0.0;
        }

        output
    }

    pub fn is_playing(&self) -> bool {
        !self.phases.is_empty()
    }

    pub fn is_visually_alive(&self, unit_id: UnitId) -> bool {
        self.visual_alive.contains(&unit_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::{Faction, UnitKind};

    #[test]
    fn turn_event_move_creation() {
        let event = TurnEvent::Move {
            unit_id: 1,
            from: (5, 5),
            to: (6, 5),
        };
        match event {
            TurnEvent::Move {
                unit_id,
                from,
                to,
            } => {
                assert_eq!(unit_id, 1);
                assert_eq!(from, (5, 5));
                assert_eq!(to, (6, 5));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn ease_out_quad_boundaries() {
        assert!((ease_out_quad(0.0)).abs() < f32::EPSILON);
        assert!((ease_out_quad(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ease_out_quad_midpoint() {
        let mid = ease_out_quad(0.5);
        assert!(
            mid > 0.5,
            "ease_out should be > linear at midpoint, got {mid}"
        );
        assert!((mid - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn animator_not_playing_when_empty() {
        let animator = TurnAnimator::new();
        assert!(!animator.is_playing());
    }

    #[test]
    fn animator_plays_move_events() {
        let mut animator = TurnAnimator::new();
        let mut units = vec![Unit::new(1, UnitKind::Warrior, Faction::Blue, 6, 5, true)];
        let (fx, fy) = grid::grid_to_world(5, 5);
        units[0].visual_x = fx;
        units[0].visual_y = fy;

        let events = vec![TurnEvent::Move {
            unit_id: 1,
            from: (5, 5),
            to: (6, 5),
        }];
        animator.enqueue(events, false);
        assert!(animator.is_playing());

        let _output = animator.update(0.3, &mut units);
        assert!(!animator.is_playing());

        let (ex, ey) = grid::grid_to_world(6, 5);
        assert!((units[0].visual_x - ex).abs() < 1.0);
        assert!((units[0].visual_y - ey).abs() < 1.0);
    }

    #[test]
    fn animator_staggered_ai_moves() {
        let mut animator = TurnAnimator::new();
        let mut units = vec![
            Unit::new(1, UnitKind::Warrior, Faction::Blue, 6, 5, true),
            Unit::new(2, UnitKind::Warrior, Faction::Red, 9, 5, false),
            Unit::new(3, UnitKind::Warrior, Faction::Red, 8, 8, false),
        ];
        let (f1x, f1y) = grid::grid_to_world(5, 5);
        units[0].visual_x = f1x;
        units[0].visual_y = f1y;
        let (f2x, f2y) = grid::grid_to_world(10, 5);
        units[1].visual_x = f2x;
        units[1].visual_y = f2y;
        let (f3x, f3y) = grid::grid_to_world(9, 8);
        units[2].visual_x = f3x;
        units[2].visual_y = f3y;

        let events = vec![
            TurnEvent::Move {
                unit_id: 1,
                from: (5, 5),
                to: (6, 5),
            },
            TurnEvent::Move {
                unit_id: 2,
                from: (10, 5),
                to: (9, 5),
            },
            TurnEvent::Move {
                unit_id: 3,
                from: (9, 8),
                to: (8, 8),
            },
        ];
        animator.enqueue(events, false);
        assert!(animator.is_playing());

        // total_duration = max(0.0+0.25, 0.05+0.25, 0.10+0.25) = 0.35
        let _output = animator.update(0.4, &mut units);
        assert!(!animator.is_playing());
    }

    #[test]
    fn animator_melee_attack_phase() {
        let mut animator = TurnAnimator::new();
        let mut units = vec![
            Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, true),
            Unit::new(2, UnitKind::Warrior, Faction::Red, 6, 5, false),
        ];

        let events = vec![TurnEvent::MeleeAttack {
            attacker_id: 1,
            defender_id: 2,
            damage: 3,
            killed: false,
        }];
        animator.enqueue(events, false);
        assert!(animator.is_playing());

        let output = animator.update(0.4, &mut units);
        assert!(!animator.is_playing());
        assert!(!output.particles.is_empty());
    }

    #[test]
    fn animator_visual_alive_tracks_kills() {
        let mut animator = TurnAnimator::new();
        let mut units = vec![
            Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, true),
            Unit::new(2, UnitKind::Warrior, Faction::Red, 6, 5, false),
        ];
        units[1].alive = false;
        units[1].death_fade = 0.3;

        animator.init_visual_alive([1, 2].into_iter());

        let events = vec![TurnEvent::MeleeAttack {
            attacker_id: 1,
            defender_id: 2,
            damage: 10,
            killed: true,
        }];
        animator.enqueue(events, false);

        assert!(animator.is_visually_alive(2));

        let _output = animator.update(0.4, &mut units);
        assert!(!animator.is_visually_alive(2));
    }
}
