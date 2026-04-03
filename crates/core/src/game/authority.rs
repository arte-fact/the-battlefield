use super::*;

impl Game {
    /// Follow chance based on authority.
    pub fn authority_follow_chance(&self) -> f32 {
        (self.config.authority_follow_base + self.authority * self.config.authority_follow_slope)
            .clamp(0.0, 1.0)
    }

    /// Command radius in world pixels based on authority.
    pub fn authority_command_radius(&self) -> f32 {
        TILE_SIZE
            * (self.config.authority_radius_base_tiles
                + self.authority * self.config.authority_radius_slope)
    }

    /// Maximum number of followers based on authority.
    pub fn authority_max_followers(&self) -> usize {
        self.config.authority_max_followers_base as usize
            + (self.authority * self.config.authority_max_followers_slope) as usize
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

    /// Check if a unit is within the player's personal FOV radius.
    fn is_unit_in_fov(&self, unit_id: UnitId) -> bool {
        let Some(player) = self.player_unit() else {
            return false;
        };
        let (px, py) = (player.x, player.y);
        if let Some(u) = self.units.iter().find(|u| u.id == unit_id) {
            let dx = u.x - px;
            let dy = u.y - py;
            let dist_sq = dx * dx + dy * dy;
            let range = self.config.rep_fov_tiles * TILE_SIZE;
            dist_sq <= range * range
        } else {
            false
        }
    }

    /// Check if a grid position is within the player's personal FOV radius.
    pub(super) fn is_tile_in_fov(&self, gx: u32, gy: u32) -> bool {
        let Some(player) = self.player_unit() else {
            return false;
        };
        let (px, py) = (player.x, player.y);
        let (wx, wy) = grid::grid_to_world(gx, gy);
        let dx = wx - px;
        let dy = wy - py;
        let dist_sq = dx * dx + dy * dy;
        let range = self.config.rep_fov_tiles * TILE_SIZE;
        dist_sq <= range * range
    }

    /// Update authority based on combat events this frame.
    pub(super) fn tick_authority(&mut self) {
        let player_id = match self.player_unit() {
            Some(p) => p.id,
            None => return,
        };

        // Collect (delta, unit_id) pairs first to avoid borrow conflicts.
        let mut deltas: Vec<(f32, UnitId)> = Vec::new();

        for event in &self.turn_events {
            match event {
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
                        deltas.push((self.config.rep_kill, *defender_id));
                    } else if self
                        .units
                        .iter()
                        .any(|u| u.id == *attacker_id && u.faction == Faction::Blue)
                        && self.is_unit_in_fov(*defender_id)
                    {
                        deltas.push((self.config.rep_ally_kill, *defender_id));
                    }
                }
                TurnEvent::MeleeAttack {
                    attacker_id,
                    defender_id,
                    killed: false,
                    ..
                }
                | TurnEvent::RangedAttack {
                    attacker_id,
                    defender_id,
                    killed: false,
                    ..
                } => {
                    if *attacker_id == player_id {
                        deltas.push((self.config.rep_hit, *defender_id));
                    }
                }
                _ => {}
            }
        }

        // Blue ally deaths
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
                deltas.push((self.config.rep_ally_death, defender_id));
            }
        }

        for (delta, uid) in deltas {
            self.apply_authority(delta, uid);
        }
    }

    /// Apply an authority delta and spawn a floating text at the unit's position.
    fn apply_authority(&mut self, delta: f32, unit_id: UnitId) {
        let old = self.authority;
        self.authority = (self.authority + delta).clamp(0.0, 100.0);
        let actual = self.authority - old;
        if actual.abs() < f32::EPSILON {
            return;
        }
        if let Some(u) = self.units.iter().find(|u| u.id == unit_id) {
            self.floating_texts.push(super::FloatingText {
                x: u.x,
                y: u.y,
                value: actual,
                remaining: super::FLOATING_TEXT_DURATION,
            });
        }
    }

    fn apply_authority_at(&mut self, delta: f32, x: f32, y: f32) {
        let old = self.authority;
        self.authority = (self.authority + delta).clamp(0.0, 100.0);
        let actual = self.authority - old;
        if actual.abs() < f32::EPSILON {
            return;
        }
        self.floating_texts.push(super::FloatingText {
            x,
            y,
            value: actual,
            remaining: super::FLOATING_TEXT_DURATION,
        });
    }

    /// Called when a zone is captured by Blue.
    pub(super) fn on_zone_captured(&mut self, in_fov: bool, x: f32, y: f32) {
        if in_fov {
            self.apply_authority_at(self.config.rep_zone_cap, x, y);
        }
    }

    /// Called when a Red zone is decaptured.
    pub(super) fn on_zone_decaptured(&mut self, in_fov: bool, x: f32, y: f32) {
        if in_fov {
            self.apply_authority_at(self.config.rep_zone_decap, x, y);
        }
    }

    /// Called when Blue loses a zone.
    pub(super) fn on_zone_lost(&mut self, in_fov: bool, x: f32, y: f32) {
        if in_fov {
            self.apply_authority_at(self.config.rep_zone_lost, x, y);
        }
    }
}
