use std::collections::HashSet;

use crate::particle::ParticleKind;
use crate::unit::{Unit, UnitAnim, UnitId};

// Timing constants
const MELEE_ATTACK_DURATION: f32 = 0.35;
const RANGED_ATTACK_DURATION: f32 = 0.5;

/// Events recorded during game logic execution for animation playback.
#[derive(Clone, Debug)]
pub enum TurnEvent {
    Move {
        unit_id: UnitId,
        from: (f32, f32),
        to: (f32, f32),
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
        target_pos: (f32, f32),
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
}

impl AnimOutput {
    fn new() -> Self {
        Self {
            particles: Vec::new(),
        }
    }
}

struct ActiveAnim {
    phase: AnimPhase,
    elapsed: f32,
}

pub struct TurnAnimator {
    anims: Vec<ActiveAnim>,
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
            anims: Vec::new(),
            visual_alive: HashSet::new(),
        }
    }

    /// Process turn events: spawn dust particles for moves, enqueue attack phases (parallel).
    pub fn enqueue(&mut self, events: Vec<TurnEvent>) -> AnimOutput {
        let mut output = AnimOutput::new();

        for event in events {
            match event {
                TurnEvent::Move {
                    unit_id: _,
                    from,
                    to: _,
                } => {
                    output.particles.push((ParticleKind::Dust, from.0, from.1));
                }
                TurnEvent::MeleeAttack {
                    attacker_id,
                    defender_id,
                    damage,
                    killed,
                } => {
                    self.anims.push(ActiveAnim {
                        phase: AnimPhase::MeleeAttack {
                            attacker_id,
                            defender_id,
                            damage,
                            killed,
                            duration: MELEE_ATTACK_DURATION,
                            particle_spawned: false,
                        },
                        elapsed: 0.0,
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
                    self.anims.push(ActiveAnim {
                        phase: AnimPhase::RangedAttack {
                            attacker_id,
                            defender_id,
                            target_pos,
                            missed,
                            damage,
                            killed,
                            duration: RANGED_ATTACK_DURATION,
                        },
                        elapsed: 0.0,
                    });
                }
                TurnEvent::Heal {
                    healer_id,
                    target_id,
                    amount: _,
                } => {
                    self.anims.push(ActiveAnim {
                        phase: AnimPhase::Heal {
                            healer_id,
                            target_id,
                            duration: MELEE_ATTACK_DURATION,
                        },
                        elapsed: 0.0,
                    });
                }
            }
        }

        output
    }

    /// Initialize the set of visually alive units before enqueueing events.
    pub fn init_visual_alive(&mut self, alive_ids: impl Iterator<Item = UnitId>) {
        self.visual_alive.clear();
        self.visual_alive.extend(alive_ids);
    }

    /// Tick ALL active animations simultaneously, removing finished ones.
    pub fn update(&mut self, dt: f32, units: &mut [Unit]) -> AnimOutput {
        let mut output = AnimOutput::new();

        if self.anims.is_empty() {
            return output;
        }

        let mut finished_indices = Vec::new();

        for (i, anim) in self.anims.iter_mut().enumerate() {
            anim.elapsed += dt;

            let done = match &mut anim.phase {
                AnimPhase::MeleeAttack {
                    attacker_id,
                    defender_id,
                    killed,
                    duration,
                    particle_spawned,
                    ..
                } => {
                    let attacker_id = *attacker_id;
                    let defender_id = *defender_id;
                    let killed = *killed;
                    let duration = *duration;

                    if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                        unit.set_anim(UnitAnim::Attack);
                    }

                    if !*particle_spawned && anim.elapsed >= duration * 0.6 {
                        *particle_spawned = true;
                        if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                            output
                                .particles
                                .push((ParticleKind::Dust, defender.x, defender.y));
                        }
                    }

                    let done = anim.elapsed >= duration;
                    if done {
                        if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                            unit.set_anim(UnitAnim::Idle);
                        }
                        if killed {
                            if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                                output.particles.push((
                                    ParticleKind::ExplosionLarge,
                                    defender.x,
                                    defender.y,
                                ));
                            }
                        }
                    }
                    done
                }
                AnimPhase::RangedAttack {
                    attacker_id,
                    defender_id,
                    target_pos,
                    missed,
                    killed,
                    duration,
                    ..
                } => {
                    let attacker_id = *attacker_id;
                    let defender_id = *defender_id;
                    let target_pos = *target_pos;
                    let missed = *missed;
                    let killed = *killed;
                    let duration = *duration;

                    if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                        unit.set_anim(UnitAnim::Attack);
                    }

                    // Projectile is now spawned directly by execute_attack() with
                    // ballistic arc + damage payload — no need to spawn here.

                    let done = anim.elapsed >= duration;
                    if done {
                        if let Some(unit) = units.iter_mut().find(|u| u.id == attacker_id) {
                            unit.set_anim(UnitAnim::Idle);
                        }
                        if missed {
                            output
                                .particles
                                .push((ParticleKind::Dust, target_pos.0, target_pos.1));
                        } else if killed {
                            if let Some(defender) = units.iter().find(|u| u.id == defender_id) {
                                output.particles.push((
                                    ParticleKind::ExplosionLarge,
                                    defender.x,
                                    defender.y,
                                ));
                            }
                        }
                    }
                    done
                }
                AnimPhase::Heal {
                    healer_id,
                    duration,
                    ..
                } => {
                    let healer_id = *healer_id;
                    let duration = *duration;

                    if let Some(unit) = units.iter_mut().find(|u| u.id == healer_id) {
                        unit.set_anim(UnitAnim::Attack);
                    }

                    let done = anim.elapsed >= duration;
                    if done {
                        if let Some(unit) = units.iter_mut().find(|u| u.id == healer_id) {
                            unit.set_anim(UnitAnim::Idle);
                        }
                    }
                    done
                }
            };

            if done {
                // Track kills for visual_alive in finished anims
                match &anim.phase {
                    AnimPhase::MeleeAttack {
                        defender_id,
                        killed: true,
                        ..
                    } => {
                        self.visual_alive.remove(defender_id);
                    }
                    AnimPhase::RangedAttack {
                        defender_id,
                        killed: true,
                        missed: false,
                        ..
                    } => {
                        self.visual_alive.remove(defender_id);
                    }
                    _ => {}
                }
                finished_indices.push(i);
            }
        }

        // Remove finished in reverse order to preserve indices
        for &i in finished_indices.iter().rev() {
            self.anims.swap_remove(i);
        }

        output
    }

    pub fn is_playing(&self) -> bool {
        !self.anims.is_empty()
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
            from: (352.0, 352.0),
            to: (416.0, 352.0),
        };
        match event {
            TurnEvent::Move { unit_id, from, to } => {
                assert_eq!(unit_id, 1);
                assert!((from.0 - 352.0).abs() < f32::EPSILON);
                assert!((to.0 - 416.0).abs() < f32::EPSILON);
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
            from: (352.0, 352.0),
            to: (416.0, 352.0),
        }];
        let output = animator.enqueue(events);
        assert!(!output.particles.is_empty());
        assert!(matches!(output.particles[0].0, ParticleKind::Dust));
        // No phases queued for movement
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
