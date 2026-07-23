use battlefield_core::game::Game;
use battlefield_core::player_input::PlayerInput;
use battlefield_core::unit::{Faction, OrderKind};

fn run(seed: u32, mode: &str) -> (Option<Faction>, f32, usize) {
    let mut game = Game::new(960.0, 640.0);
    game.config.enemy_count = 1;
    if mode == "none" {
        game.player_faction = None; // unaligned observer: pure AI vs AI
    }
    game.setup_demo_battle_with_seed(seed);
    let input = match mode {
        "wander" => PlayerInput {
            move_x: 0.5,
            move_y: 0.3,
            ..Default::default()
        },
        _ => PlayerInput::default(),
    };
    let dt = 1.0 / 60.0;
    let mut max_followers = 0usize;
    for frame in 0..400_000 {
        game.tick(&input, dt);
        if frame % 300 == 0 {
            let f = game
                .units
                .iter()
                .filter(|u| {
                    u.alive && u.faction == Faction::Blue && u.order == Some(OrderKind::Follow)
                })
                .count();
            max_followers = max_followers.max(f);
        }
        if game.winner.is_some() {
            return (game.winner, frame as f32 * dt, max_followers);
        }
        // Observer mode: the pawn dying (stray hits) must not end the probe
        if mode == "none" && !game.is_player_alive() {
            // keep going; winner still resolves by conquest
        }
    }
    (None, 400_000.0 * dt, max_followers)
}

fn main() {
    for mode in ["wander", "still", "none"] {
        let mut blue = 0;
        let mut red = 0;
        let mut other = 0;
        let mut durs = Vec::new();
        let mut fmax = 0;
        for seed in [7u32, 42, 123, 777, 1234, 9999] {
            let (w, dur, mf) = run(seed, mode);
            durs.push(dur as u32);
            fmax = fmax.max(mf);
            match w {
                Some(Faction::Blue) => blue += 1,
                Some(Faction::Red) => red += 1,
                _ => other += 1,
            }
        }
        println!(
            "{mode:>7}: Blue {blue} / Red {red} / none {other}  durations={durs:?} max_followers={fmax}"
        );
    }
}
