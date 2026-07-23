use battlefield_core::game::Game;
use battlefield_core::player_input::PlayerInput;
use battlefield_core::unit::Faction;

fn main() {
    let mut blue = 0;
    let mut red = 0;
    let mut stall = 0;
    let mut bias_sum = 0.0f32;
    for seed in [
        7u32, 21, 42, 77, 123, 321, 777, 1234, 4242, 5555, 9999, 31337,
    ] {
        let mut game = Game::new(960.0, 640.0);
        game.config.enemy_count = 1;
        game.player_faction = None;
        game.setup_demo_battle_with_seed(seed);
        let (bx, by) = {
            let z = &game.zone_manager.zones[0];
            (z.center_wx, z.center_wy)
        };
        let (rx, ry) = {
            let z = &game.zone_manager.zones[1];
            (z.center_wx, z.center_wy)
        };
        let input = PlayerInput::default();
        let dt = 1.0 / 60.0;
        let mut front_bias = 0.0f32; // + = fighting nearer RED capital
        let mut samples = 0u32;
        let mut result = None;
        for frame in 0..400_000u32 {
            game.tick(&input, dt);
            if frame % 120 == 0 {
                let fighters: Vec<(f32, f32)> = game
                    .units
                    .iter()
                    .filter(|u| u.alive && u.hit_flash > 0.0)
                    .map(|u| (u.x, u.y))
                    .collect();
                if !fighters.is_empty() {
                    let (cx, cy) = (
                        fighters.iter().map(|f| f.0).sum::<f32>() / fighters.len() as f32,
                        fighters.iter().map(|f| f.1).sum::<f32>() / fighters.len() as f32,
                    );
                    let db = ((cx - bx).powi(2) + (cy - by).powi(2)).sqrt();
                    let dr = ((cx - rx).powi(2) + (cy - ry).powi(2)).sqrt();
                    // -1 = at Blue's gates, +1 = at Red's gates
                    front_bias += (db - dr) / (db + dr).max(1.0);
                    samples += 1;
                }
            }
            if game.winner.is_some() {
                result = game.winner;
                break;
            }
        }
        let bias = if samples > 0 {
            front_bias / samples as f32
        } else {
            0.0
        };
        bias_sum += bias;
        match result {
            Some(Faction::Blue) => blue += 1,
            Some(Faction::Red) => red += 1,
            _ => stall += 1,
        }
        println!("seed {seed}: winner={result:?} front_bias={bias:+.2}");
    }
    println!(
        "TOTAL Blue {blue} / Red {red} / stall {stall}  avg_front_bias={:+.3} (- = war on Blue's side)",
        bias_sum / 12.0
    );
}
