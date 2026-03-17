use std::collections::{HashSet, VecDeque};

use crate::grid;
use crate::particle::ParticleKind;
use crate::unit::{Unit, UnitAnim, UnitId};

// Timing constants
const MELEE_ATTACK_DURATION: f32 = 0.35;
const RANGED_ATTACK_DURATION: f32 = 0.5;

/// Speed of visual lerp toward grid position (exponential decay factor per second).
pub const VISUAL_LERP_SPEED: f32 = 12.0;

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
    Heal {
        healer_id: UnitId,
        target_id: UnitId,
        amount: i32,
    },
}

/// Easing: deceleration curve.
pub fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Lerp all unit visual positions toward their grid positions.
/// Call every frame. Units slide smoothly to where they are on the grid.
pub fn lerp_visual_positions(units: &mut [Unit], dt: f32) {
    let t = (dt * VISUAL_LERP_SPEED).min(1.0);
    for unit in units.iter_mut() {
        let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
        let dx = wx - unit.visual_x;
        let dy = wy - unit.visual_y;
        // If very close, snap to avoid perpetual tiny drift
        if dx.abs() < 0.5 && dy.abs() < 0.5 {
            unit.visual_x = wx;
            unit.visual_y = wy;
            // If was running, switch back to idle
            if unit.current_anim == UnitAnim::Run {
                unit.set_anim(UnitAnim::Idle);
            }
        } else {
            unit.visual_x += dx * t;
            unit.visual_y += dy * t;
            // Set run animation while moving
            if unit.alive && unit.current_anim == UnitAnim::Idle {
                unit.set_anim(UnitAnim::Run);
            }
        }
    }
}

pub enum AnimPhase {
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
    Heal {
        healer_id: UnitId,
        #[allow(dead_code)]
        target_id: UnitId,
        duration: f32,
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

    /// Process turn events: spawn dust particles for moves, enqueue attack phases.
    pub fn enqueue(&mut self, events: Vec<TurnEvent>) -> AnimOutput {
        let mut output = AnimOutput::new();

        for event in events {
            match event {
                TurnEvent::Move { unit_id: _, from, to: _ } => {
                    // Movement is handled by per-frame lerp. Just spawn dust.
                    let (fx, fy) = grid::grid_to_world(from.0, from.1);
                    output.particles.push((ParticleKind::Dust, fx, fy));
                }
                TurnEvent::MeleeAttack {
                    attacker_id,
                    defender_id,
                    damage,
                    killed,
                } => {
                    self.phases.push_back(AnimPhase::MeleeAttack {
                        attacker_id,
                        defender_id,
                        damage,
                        killed,
                        duration: MELEE_ATTACK_DURATION,
                        particle_spawned: false,
                    });
                }
                TurnEvent::RangedAttack {
                    attacker_id,
                    defender_id,
                    damage,
                    killed,
                    target_pos,
                    missed,
                } => {
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
                TurnEvent::Heal {
                    healer_id,
                    target_id,
                    amount: _,
                } => {
                    self.phases.push_back(AnimPhase::Heal {
                        healer_id,
                        target_id,
                        duration: MELEE_ATTACK_DURATION,
                    });
                }
            }
        }

        if !self.phases.is_empty() {
            self.phase_elapsed = 0.0;
        }

        output
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
            Some(AnimPhase::Heal {
                healer_id,
                duration,
                ..
            }) => {
                let healer_id = *healer_id;
                let duration = *duration;

                if let Some(unit) = units.iter_mut().find(|u| u.id == healer_id) {
                    unit.set_anim(UnitAnim::Attack);
                }

                let done = self.phase_elapsed >= duration;
                if done {
                    if let Some(unit) = units.iter_mut().find(|u| u.id == healer_id) {
                        unit.set_anim(UnitAnim::Idle);
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
    fn move_event_spawns_dust() {
        let mut animator = TurnAnimator::new();
        let events = vec![TurnEvent::Move {
            unit_id: 1,
            from: (5, 5),
            to: (6, 5),
        }];
        let output = animator.enqueue(events);
        assert!(!output.particles.is_empty());
        assert!(matches!(output.particles[0].0, ParticleKind::Dust));
        // No phases queued for movement
        assert!(!animator.is_playing());
    }

    #[test]
    fn lerp_visual_positions_snaps_when_close() {
        let mut units = vec![Unit::new(1, UnitKind::Warrior, Faction::Blue, 5, 5, true)];
        let (wx, wy) = grid::grid_to_world(5, 5);
        units[0].visual_x = wx + 0.1;
        units[0].visual_y = wy + 0.1;

        lerp_visual_positions(&mut units, 0.016);
        assert!((units[0].visual_x - wx).abs() < f32::EPSILON);
        assert!((units[0].visual_y - wy).abs() < f32::EPSILON);
    }

    #[test]
    fn lerp_visual_positions_moves_toward_target() {
        let mut units = vec![Unit::new(1, UnitKind::Warrior, Faction::Blue, 6, 5, true)];
        // Start visual at grid (5,5), but unit is at grid (6,5)
        let (fx, fy) = grid::grid_to_world(5, 5);
        units[0].visual_x = fx;
        units[0].visual_y = fy;

        let (tx, _ty) = grid::grid_to_world(6, 5);

        // After one frame, should be closer to target
        lerp_visual_positions(&mut units, 0.016);
        assert!(units[0].visual_x > fx);
        assert!(units[0].visual_x < tx);
        assert_eq!(units[0].current_anim, UnitAnim::Run);

        // After many frames, should reach target
        for _ in 0..100 {
            lerp_visual_positions(&mut units, 0.016);
        }
        assert!((units[0].visual_x - tx).abs() < f32::EPSILON);
        assert_eq!(units[0].current_anim, UnitAnim::Idle);
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
        animator.enqueue(events);
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
        animator.enqueue(events);

        assert!(animator.is_visually_alive(2));

        let _output = animator.update(0.4, &mut units);
        assert!(!animator.is_visually_alive(2));
    }
}
