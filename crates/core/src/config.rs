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
    pub zone_lock_duration: f32,
    pub combat_target_commit_secs: f32,
    pub combat_disengage_margin_tiles: f32,
    pub kite_hysteresis_tiles: f32,
    pub lancer_backoff_hysteresis_tiles: f32,
    pub follow_deadband_tiles: f32,
    pub zone_idle_margin_tiles: f32,
    pub separation_smoothing: f32,
    pub fov_radius: i32,
    pub move_speed_divisor: f32,
    /// Seconds a unit ignores out-of-reach enemies after failing to path to one.
    #[serde(default = "default_chase_block_secs")]
    pub chase_block_secs: f32,

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
    pub order_follow_duration: f32,
    pub order_charge_timeout: f32,
    pub order_defend_duration: f32,
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
    /// Distance beyond the zone radius a garrison pursues intruders.
    #[serde(default = "default_zone_defend_leash_tiles")]
    pub zone_defend_leash_tiles: f32,
    /// Extra zone score for zones held by the settlement leader (the
    /// pack turns on whoever is winning).
    #[serde(default = "default_ai_w_leader")]
    pub ai_w_leader: f32,
    /// Extra zone score for zones road-adjacent to own territory.
    #[serde(default = "default_ai_w_neighbor")]
    pub ai_w_neighbor: f32,

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
    /// Side length of the playable map area in tiles (border added on top).
    #[serde(default = "default_playable_size")]
    pub playable_size: u32,
    /// Max peon deliveries a village can bank.
    #[serde(default = "default_village_stock_cap")]
    pub village_stock_cap: u8,
    /// AI opponents in the battle (1-3): Red, +Yellow, +Purple.
    #[serde(default = "default_enemy_count")]
    pub enemy_count: u8,
    /// Standing garrison size a village maintains (per village).
    #[serde(default = "default_garrison_cap")]
    pub garrison_cap: u8,
    /// Seconds between garrison spawn attempts per village.
    #[serde(default = "default_garrison_spawn_interval")]
    pub garrison_spawn_interval: f32,

    // ── Retinue / Recruitment ───────────────────────────────────────
    /// Seconds between auto-recruitment passes.
    #[serde(default = "default_recruit_interval")]
    pub recruit_interval: f32,
    /// Distance (tiles) beyond which a follower starts losing contact.
    #[serde(default = "default_recruit_leash_tiles")]
    pub recruit_leash_tiles: f32,
    /// Seconds out of contact before a follower is released to the army.
    #[serde(default = "default_recruit_lost_contact_secs")]
    pub recruit_lost_contact_secs: f32,
    /// Seconds a dismissed unit refuses re-recruitment.
    #[serde(default = "default_re_recruit_cooldown_secs")]
    pub re_recruit_cooldown_secs: f32,

    // ── Manpower / Conquest ─────────────────────────────────────────
    /// Reinforcements each faction can field over the battle (spawns cost 1).
    #[serde(default = "default_manpower_start")]
    pub manpower_start: f32,
    /// Zones a faction must control before the enemy pool starts bleeding.
    #[serde(default = "default_bleed_zone_threshold")]
    pub bleed_zone_threshold: usize,
    /// Enemy manpower drained per second per zone at or above the threshold.
    #[serde(default = "default_bleed_per_extra_zone")]
    pub bleed_per_extra_zone: f32,
}

fn default_chase_block_secs() -> f32 {
    5.0
}

fn default_zone_defend_leash_tiles() -> f32 {
    4.0
}

fn default_ai_w_leader() -> f32 {
    8.0
}

fn default_ai_w_neighbor() -> f32 {
    6.0
}

fn default_playable_size() -> u32 {
    crate::grid::PLAYABLE_SIZE
}

fn default_village_stock_cap() -> u8 {
    5
}

fn default_enemy_count() -> u8 {
    1
}

fn default_garrison_cap() -> u8 {
    4
}

fn default_garrison_spawn_interval() -> f32 {
    6.0
}

fn default_recruit_interval() -> f32 {
    1.0
}

fn default_recruit_leash_tiles() -> f32 {
    15.0
}

fn default_recruit_lost_contact_secs() -> f32 {
    3.0
}

fn default_re_recruit_cooldown_secs() -> f32 {
    12.0
}

fn default_manpower_start() -> f32 {
    300.0
}

fn default_bleed_zone_threshold() -> usize {
    4
}

fn default_bleed_per_extra_zone() -> f32 {
    0.25
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
            knockback_dist_tiles: 0.2,
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
            zone_lock_duration: 5.0,
            combat_target_commit_secs: 1.0,
            combat_disengage_margin_tiles: 2.0,
            kite_hysteresis_tiles: 0.75,
            lancer_backoff_hysteresis_tiles: 0.5,
            follow_deadband_tiles: 0.5,
            zone_idle_margin_tiles: 1.5,
            separation_smoothing: 0.3,
            fov_radius: 15,
            move_speed_divisor: 0.90,
            chase_block_secs: default_chase_block_secs(),

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
            order_follow_duration: 15.0,
            order_charge_timeout: 10.0,
            order_defend_duration: 30.0,
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
            zone_defend_leash_tiles: default_zone_defend_leash_tiles(),
            ai_w_leader: default_ai_w_leader(),
            ai_w_neighbor: default_ai_w_neighbor(),

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
            zone_radius: 6,
            victory_hold_time: 60.0,
            playable_size: default_playable_size(),
            village_stock_cap: default_village_stock_cap(),
            enemy_count: default_enemy_count(),
            garrison_cap: default_garrison_cap(),
            garrison_spawn_interval: default_garrison_spawn_interval(),

            // Retinue / Recruitment
            recruit_interval: default_recruit_interval(),
            recruit_leash_tiles: default_recruit_leash_tiles(),
            recruit_lost_contact_secs: default_recruit_lost_contact_secs(),
            re_recruit_cooldown_secs: default_re_recruit_cooldown_secs(),

            // Manpower / Conquest
            manpower_start: default_manpower_start(),
            bleed_zone_threshold: default_bleed_zone_threshold(),
            bleed_per_extra_zone: default_bleed_per_extra_zone(),
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
