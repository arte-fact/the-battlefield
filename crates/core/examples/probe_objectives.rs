use battlefield_core::game::Game;
use battlefield_core::player_input::PlayerInput;

fn main() {
    let mut game = Game::new(1280.0, 800.0);
    game.setup_demo_battle_with_seed(42);
    let input = PlayerInput::default();
    let dt = 1.0 / 60.0;

    let mut last_pick: Option<(Option<u8>, Option<u8>)> = None;
    let mut changes = 0u32;
    for frame in 0..(300 * 60) {
        let t = frame as f32 / 60.0;
        game.tick(&input, dt);
        game.update(dt as f64);
        if game.winner.is_some() {
            break;
        }
        // Recompute Blue's defend/attack picks the same way the planner does
        let objectives = &game.macro_objectives[0];
        let mut defend: Option<u8> = None;
        let mut attack: Option<u8> = None;
        let mut scores = Vec::new();
        for &(wx, wy, score) in objectives {
            let zi = game
                .zone_manager
                .zones
                .iter()
                .position(|z| (z.center_wx - wx).abs() < 1.0 && (z.center_wy - wy).abs() < 1.0);
            let Some(zi) = zi else { continue };
            scores.push((zi as u8, score));
            if score >= 200.0 && defend.is_none() {
                defend = Some(zi as u8);
            } else if score >= 85.0 && attack.is_none() {
                attack = Some(zi as u8);
            }
        }
        let _ = (defend, attack);
        let pick = game.planner_targets[0];
        if frame % 6 == 0 && last_pick.is_some_and(|p| p != pick) {
            changes += 1;
            let states: Vec<String> = game
                .zone_manager
                .zones
                .iter()
                .map(|z| format!("{:?}:{:.2}", z.id, z.progress))
                .collect();
            println!(
                "t={t:>5.1} pick {:?} -> {:?}   scores={:?}",
                last_pick.unwrap(),
                pick,
                scores
                    .iter()
                    .map(|(z, s)| format!("z{z}={s:.0}"))
                    .collect::<Vec<_>>()
            );
            let _ = states;
        }
        if last_pick.is_none() || frame % 6 == 0 {
            last_pick = Some(pick);
        }
    }
    println!("total pick changes: {changes}");
}
