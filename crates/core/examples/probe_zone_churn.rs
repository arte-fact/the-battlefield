use battlefield_core::game::Game;
use battlefield_core::grid::TILE_SIZE;
use battlefield_core::player_input::PlayerInput;
use battlefield_core::unit::Faction;
use battlefield_core::zone::ZoneState;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
enum Where {
    Out,
    In,
}

fn main() {
    let mut game = Game::new(1280.0, 800.0);
    game.setup_demo_battle_with_seed(42);
    let input = PlayerInput::default();
    let dt = 1.0 / 60.0;

    // (unit, zone) -> (state, last_transition_time, entry_count_in_window)
    let mut loc: HashMap<(u32, u8), (Where, f32, Vec<f32>)> = HashMap::new();
    let mut events: Vec<String> = Vec::new();
    let mut hp_at_entry: HashMap<(u32, u8), i32> = HashMap::new();
    let mut state_changes = vec![0u32; game.zone_manager.zones.len()];
    let mut last_states: Vec<_> = game.zone_manager.zones.iter().map(|z| z.state).collect();

    for frame in 0..(300 * 60) {
        let t = frame as f32 / 60.0;
        game.tick(&input, dt);
        game.update(dt as f64);
        if game.winner.is_some() {
            events.push(format!("t={t:.0} battle ended"));
            break;
        }
        for (zi, z) in game.zone_manager.zones.iter().enumerate() {
            if z.state != last_states[zi] {
                state_changes[zi] += 1;
                last_states[zi] = z.state;
            }
        }
        for u in game
            .units
            .iter()
            .filter(|u| u.alive && !u.is_player && u.faction == Faction::Blue)
        {
            let Some(az) = u.assigned_zone else { continue };
            let z = &game.zone_manager.zones[az as usize];
            if z.state == ZoneState::Controlled(Faction::Blue) {
                continue; // only uncaptured/contested zones
            }
            let r = z.radius as f32 * TILE_SIZE;
            let d = ((u.x - z.center_wx).powi(2) + (u.y - z.center_wy).powi(2)).sqrt();
            let now = if d < r { Where::In } else { Where::Out };
            let key = (u.id, az);
            let entry = loc.entry(key).or_insert((now, t, Vec::new()));
            if entry.0 != now {
                if now == Where::In {
                    entry.2.push(t);
                    hp_at_entry.insert(key, u.hp);
                    // 3 entries into the same uncaptured zone within 30s = churn
                    let recent: Vec<f32> =
                        entry.2.iter().copied().filter(|&e| t - e < 30.0).collect();
                    if recent.len() >= 3 {
                        let no_combat = hp_at_entry.get(&key).map(|&h| h == u.hp).unwrap_or(false);
                        events.push(format!(
                            "t={t:>5.1} unit {} zone {az}: {} entries in 30s (last exits/enters: {:?}) hp={} combat_free={} target={:?}",
                            u.id,
                            recent.len(),
                            recent.iter().map(|e| format!("{e:.1}")).collect::<Vec<_>>(),
                            u.hp,
                            no_combat,
                            u.combat_target,
                        ));
                    }
                }
                *entry = (now, t, entry.2.clone());
            }
        }
    }
    println!("zone state changes over battle: {state_changes:?}");
    println!("churn events: {}", events.len());
    for e in events.iter().take(30) {
        println!("{e}");
    }
}
