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

    /// Current reputation name based on authority level.
    pub fn authority_rank_name(&self) -> &'static str {
        if self.authority >= 80.0 {
            "Legend"
        } else if self.authority >= 60.0 {
            "Hero"
        } else if self.authority >= 40.0 {
            "Veteran"
        } else if self.authority >= 20.0 {
            "Known"
        } else {
            "Unknown"
        }
    }

    /// Count alive recruited units.
    pub fn follower_count(&self) -> usize {
        self.recruited
            .iter()
            .filter(|id| self.units.iter().any(|u| u.alive && u.id == **id))
            .count()
    }

    /// Check if a unit's grid position is within the player's field of view.
    fn is_unit_in_fov(&self, unit_id: UnitId) -> bool {
        if let Some(u) = self.units.iter().find(|u| u.id == unit_id) {
            let (gx, gy) = u.grid_cell();
            let idx = (gy * self.grid.width + gx) as usize;
            idx < self.visible.len() && self.visible[idx]
        } else {
            false
        }
    }

    /// Update authority based on combat events this frame.
    ///
    /// Reputation is tied to player involvement:
    /// - Player kills/damage: always rewarded
    /// - Ally kills: only rewarded if the player can see it (FOV)
    /// - Ally deaths: only penalized if visible to the player
    pub(super) fn tick_authority(&mut self) {
        let player_id = match self.player_unit() {
            Some(p) => p.id,
            None => return,
        };

        let mut authority_delta = 0.0_f32;

        for event in &self.turn_events {
            match event {
                // Kills
                TurnEvent::MeleeAttack {
                    attacker_id,
                    defender_id,
                    killed: true,
                    ..
                }
                | TurnEvent::RangedAttack {
                    attacker_id,
                    defender_id,
                    killed: true,
                    ..
                } => {
                    if *attacker_id == player_id {
                        // Player kill — always rewarded
                        authority_delta += 3.0;
                    } else if self
                        .units
                        .iter()
                        .any(|u| u.id == *attacker_id && u.faction == Faction::Blue)
                        && self.is_unit_in_fov(*defender_id)
                    {
                        // Blue ally kill witnessed by player
                        authority_delta += 1.5;
                    }
                }
                // Player non-lethal damage
                TurnEvent::MeleeAttack {
                    attacker_id,
                    killed: false,
                    ..
                }
                | TurnEvent::RangedAttack {
                    attacker_id,
                    killed: false,
                    ..
                } => {
                    if *attacker_id == player_id {
                        authority_delta += 0.5;
                    }
                }
                _ => {}
            }
        }

        // Blue ally deaths — only penalize if player can see it
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
                && self.is_unit_in_fov(defender_id)
            {
                authority_delta -= 1.5;
            }
        }

        self.authority = (self.authority + authority_delta).clamp(0.0, 100.0);
    }

    /// Called when a zone is captured by Blue. Only grants reputation if in player FOV.
    pub(super) fn on_zone_captured(&mut self, in_fov: bool) {
        if in_fov {
            self.authority = (self.authority + 5.0).min(100.0);
        }
    }

    /// Called when a Red zone is decaptured. Only grants reputation if in player FOV.
    pub(super) fn on_zone_decaptured(&mut self, in_fov: bool) {
        if in_fov {
            self.authority = (self.authority + 3.0).min(100.0);
        }
    }

    /// Called when Blue loses a zone. Only penalizes if in player FOV.
    pub(super) fn on_zone_lost(&mut self, in_fov: bool) {
        if in_fov {
            self.authority = (self.authority - 1.5).max(0.0);
        }
    }
}
