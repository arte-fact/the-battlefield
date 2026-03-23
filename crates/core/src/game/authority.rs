use super::*;

impl Game {
    /// Follow chance based on authority: 30% at 0, 95% at 100.
    pub fn authority_follow_chance(&self) -> f32 {
        (0.30 + self.authority * 0.0065).clamp(0.0, 1.0)
    }

    /// Command radius in world pixels based on authority.
    pub fn authority_command_radius(&self) -> f32 {
        TILE_SIZE * (3.0 + self.authority * 0.09)
    }

    /// Maximum number of followers based on authority.
    pub fn authority_max_followers(&self) -> usize {
        3 + (self.authority * 0.27) as usize
    }

    /// Current rank name.
    pub fn authority_rank_name(&self) -> &'static str {
        if self.authority >= 80.0 {
            "Commander"
        } else if self.authority >= 60.0 {
            "Captain"
        } else if self.authority >= 40.0 {
            "Sergeant"
        } else if self.authority >= 20.0 {
            "Soldier"
        } else {
            "Recruit"
        }
    }

    /// Count units currently following player orders.
    pub fn follower_count(&self) -> usize {
        self.units
            .iter()
            .filter(|u| u.alive && !u.is_player && u.faction == Faction::Blue && u.order.is_some())
            .count()
    }

    /// Update authority based on combat events this frame.
    /// Called from tick() before turn_events are drained.
    pub(super) fn tick_authority(&mut self) {
        let player_id = match self.player_unit() {
            Some(p) => p.id,
            None => return,
        };

        let mut authority_delta = 0.0_f32;

        // Scan turn events for kills and heals
        for event in &self.turn_events {
            match event {
                TurnEvent::MeleeAttack {
                    attacker_id,
                    killed: true,
                    ..
                }
                | TurnEvent::RangedAttack {
                    attacker_id,
                    killed: true,
                    ..
                } => {
                    if *attacker_id == player_id {
                        authority_delta += 3.0;
                    } else if self.units.iter().any(|u| {
                        u.id == *attacker_id && u.order.is_some() && u.faction == Faction::Blue
                    }) {
                        authority_delta += 1.0;
                    } else if self
                        .units
                        .iter()
                        .any(|u| u.id == *attacker_id && u.faction == Faction::Blue)
                    {
                        authority_delta += 0.3;
                    }
                }
                TurnEvent::Heal { .. } => {
                    authority_delta += 0.5;
                }
                _ => {}
            }
        }

        // Detect Blue follower deaths from kill events in turn_events.
        // Note: unit.order is cleared on death in take_damage(), so we detect
        // follower deaths by checking if the defender_id was a Blue non-player unit
        // killed this frame.
        for event in &self.turn_events {
            let defender_id = match event {
                TurnEvent::MeleeAttack {
                    defender_id,
                    killed: true,
                    ..
                }
                | TurnEvent::RangedAttack {
                    defender_id,
                    killed: true,
                    ..
                } => *defender_id,
                _ => continue,
            };
            if self
                .units
                .iter()
                .any(|u| u.id == defender_id && u.faction == Faction::Blue && !u.is_player)
            {
                authority_delta -= 2.0;
            }
        }

        self.authority = (self.authority + authority_delta).clamp(0.0, 100.0);
    }

    /// Called when a zone is captured by Blue.
    pub(super) fn on_zone_captured(&mut self) {
        self.authority = (self.authority + 5.0).min(100.0);
    }

    /// Called when a zone is lost by Blue.
    pub(super) fn on_zone_lost(&mut self) {
        self.authority = (self.authority - 3.0).max(0.0);
    }
}
