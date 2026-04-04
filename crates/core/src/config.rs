use serde::{Deserialize, Serialize};

/// Runtime-tweakable game configuration. All distance fields suffixed `_tiles`
/// store a multiplier of `TILE_SIZE` — the usage site computes the final pixel value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameConfig {
    // ── AI Behavior ─────────────────────────────────────────────────
    pub monk_safe_dist_tiles: f32,
    pub monk_follow_dist_tiles: f32,
    pub monk_max_follow_tiles: f32,
    pub objective_interval: f32,
    pub astar_budget_per_tick: u8,
    pub ai_vision_radius: u32,
    pub knockback_dist_tiles: f32,
    pub astar_search_limit: u32,
    pub repath_cooldown_base: f32,
    pub repath_cooldown_jitter: f32,
    pub repath_cooldown_mod: f32,
    pub failed_path_cooldown: f32,
    pub deferred_repath_delay: f32,
    pub waypoint_arrival_frac: f32,
    pub separation_radius_mult: f32,
    pub flow_weight: f32,
    pub separation_weight: f32,
    pub fov_radius: i32,
    pub move_speed_divisor: f32,
    pub flow_initial_cost_base: u32,
    pub flow_score_multiplier: f32,

    // ── Zone Assignment Scoring ─────────────────────────────────────
    pub zone_cost_norm_divisor: f32,
    pub zone_distance_weight: f32,
    pub zone_congestion_weight: f32,
    pub zone_hysteresis: f32,
    pub zone_authority_influence: f32,
    pub zone_authority_radius_tiles: f32,
    pub zone_health_penalty: f32,
    pub zone_contested_bonus: f32,
    pub zone_archer_ally_bonus: f32,
    pub zone_capture_commit_base: f32,
    pub zone_capture_commit_extra_base: f32,
    pub zone_capture_commit_progress_mult: f32,
    pub zone_lock_duration: f32,

    // ── Strategic Zone Scoring ──────────────────────────────────────
    pub strat_uncontrolled: f32,
    pub strat_erosion_urgency: f32,
    pub strat_momentum: f32,
    pub strat_contested: f32,
    pub strat_enemy_pressure: f32,
    pub strat_distance_penalty: f32,
    pub strat_all_controlled_defense: f32,

    // ── Authority System ────────────────────────────────────────────
    pub authority_follow_base: f32,
    pub authority_follow_slope: f32,
    pub authority_radius_base_tiles: f32,
    pub authority_radius_slope: f32,
    pub authority_max_followers_base: u32,
    pub authority_max_followers_slope: f32,
    pub rep_kill: f32,
    pub rep_ally_kill: f32,
    pub rep_hit: f32,
    pub rep_ally_death: f32,
    pub rep_zone_cap: f32,
    pub rep_zone_decap: f32,
    pub rep_zone_lost: f32,
    pub rep_fov_tiles: f32,

    // ── Orders & Formations ─────────────────────────────────────────
    pub order_flash_duration: f32,
    pub charge_distance_tiles: f32,
    pub charge_arrival_tiles: f32,
    pub follow_distance_tiles: f32,
    pub order_leash_tiles: f32,
    pub defend_line_warrior_tiles: f32,
    pub defend_line_lancer_tiles: f32,
    pub defend_line_archer_tiles: f32,
    pub defend_line_monk_tiles: f32,
    pub defend_spacing_tiles: f32,
    pub defend_leash_melee_tiles: f32,
    pub defend_leash_ranged_tiles: f32,

    // ── Combat Balancing ─────────────────────────────────────────────
    pub warrior_hp: i32,
    pub warrior_atk: i32,
    pub warrior_def: i32,
    pub warrior_cooldown: f32,
    pub archer_hp: i32,
    pub archer_atk: i32,
    pub archer_def: i32,
    pub archer_cooldown: f32,
    pub lancer_hp: i32,
    pub lancer_atk: i32,
    pub lancer_def: i32,
    pub lancer_cooldown: f32,
    pub monk_hp: i32,
    pub monk_heal_amount: i32,
    pub monk_cooldown: f32,
    pub melee_range_tiles: f32,
    pub arrow_speed: f32,
    pub arrow_arc_base: f32,
    pub tower_damage: i32,
    pub tower_range_tiles: f32,
    pub spawn_interval: f32,

    // ── Zone & Game Rules ───────────────────────────────────────────
    pub base_capture_time: f32,
    pub max_capture_multiplier: f32,
    pub max_units_per_faction: usize,
    pub zone_radius: u32,
    pub victory_hold_time: f32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            // AI Behavior
            monk_safe_dist_tiles: 3.0,
            monk_follow_dist_tiles: 2.0,
            monk_max_follow_tiles: 15.0,
            objective_interval: 2.0,
            astar_budget_per_tick: 3,
            ai_vision_radius: 10,
            knockback_dist_tiles: 0.7,
            astar_search_limit: 40,
            repath_cooldown_base: 0.4,
            repath_cooldown_jitter: 0.17,
            repath_cooldown_mod: 0.2,
            failed_path_cooldown: 0.1,
            deferred_repath_delay: 0.05,
            waypoint_arrival_frac: 1.0 / 3.0,
            separation_radius_mult: 3.0,
            flow_weight: 0.8,
            separation_weight: 0.2,
            fov_radius: 15,
            move_speed_divisor: 0.90,
            flow_initial_cost_base: 750,
            flow_score_multiplier: 5.0,

            // Zone Assignment Scoring
            zone_cost_norm_divisor: 2500.0,
            zone_distance_weight: -30.0,
            zone_congestion_weight: -8.0,
            zone_hysteresis: 15.0,
            zone_authority_influence: 10.0,
            zone_authority_radius_tiles: 15.0,
            zone_health_penalty: 20.0,
            zone_contested_bonus: 10.0,
            zone_archer_ally_bonus: 2.0,
            zone_capture_commit_base: 20.0,
            zone_capture_commit_extra_base: 25.0,
            zone_capture_commit_progress_mult: 20.0,
            zone_lock_duration: 8.0,

            // Strategic Zone Scoring
            strat_uncontrolled: 40.0,
            strat_erosion_urgency: 50.0,
            strat_momentum: 30.0,
            strat_contested: 20.0,
            strat_enemy_pressure: 5.0,
            strat_distance_penalty: -15.0,
            strat_all_controlled_defense: 10.0,

            // Authority System
            authority_follow_base: 0.30,
            authority_follow_slope: 0.0065,
            authority_radius_base_tiles: 3.0,
            authority_radius_slope: 0.09,
            authority_max_followers_base: 3,
            authority_max_followers_slope: 0.27,
            rep_kill: 3.0,
            rep_ally_kill: 1.5,
            rep_hit: 0.5,
            rep_ally_death: -1.5,
            rep_zone_cap: 5.0,
            rep_zone_decap: 3.0,
            rep_zone_lost: -1.5,
            rep_fov_tiles: 15.0,

            // Orders & Formations
            order_flash_duration: 1.0,
            charge_distance_tiles: 8.0,
            charge_arrival_tiles: 1.5,
            follow_distance_tiles: 1.5,
            order_leash_tiles: 4.0,
            defend_line_warrior_tiles: 1.0,
            defend_line_lancer_tiles: 2.0,
            defend_line_archer_tiles: 3.0,
            defend_line_monk_tiles: 4.0,
            defend_spacing_tiles: 1.0,
            defend_leash_melee_tiles: 3.0,
            defend_leash_ranged_tiles: 8.0,

            // Combat Balancing
            warrior_hp: 10,
            warrior_atk: 3,
            warrior_def: 3,
            warrior_cooldown: 0.40,
            archer_hp: 6,
            archer_atk: 2,
            archer_def: 1,
            archer_cooldown: 0.55,
            lancer_hp: 10,
            lancer_atk: 4,
            lancer_def: 1,
            lancer_cooldown: 0.35,
            monk_hp: 5,
            monk_heal_amount: 3,
            monk_cooldown: 0.50,
            melee_range_tiles: 2.0,
            arrow_speed: 600.0,
            arrow_arc_base: 30.0,
            tower_damage: 2,
            tower_range_tiles: 7.0,
            spawn_interval: 1.5,

            // Zone & Game Rules
            base_capture_time: 16.0,
            max_capture_multiplier: 3.0,
            max_units_per_faction: 35,
            zone_radius: 4,
            victory_hold_time: 60.0,
        }
    }
}

impl GameConfig {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}
