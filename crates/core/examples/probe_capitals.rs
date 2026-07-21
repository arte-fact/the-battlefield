use battlefield_core::game::Game;

fn main() {
    let mut bad = 0;
    for &size in &[160u32, 192, 224, 384, 512] {
        for enemies in 1u8..=3 {
            for seed in [7u32, 21, 42, 77, 123, 777, 1234, 5555, 9999, 31337] {
                let mut game = Game::new(960.0, 640.0);
                game.config.playable_size = size;
                game.config.enemy_count = enemies;
                game.setup_demo_battle_with_seed(seed);
                let n_caps = 1 + enemies as usize;
                for zi in 0..n_caps {
                    let z = &game.zone_manager.zones[zi];
                    let (zx, zy) = (z.center_gx as f32, z.center_gy as f32);
                    let near: Vec<_> = game
                        .buildings
                        .iter()
                        .filter(|b| {
                            let dx = b.grid_x as f32 - zx;
                            let dy = b.grid_y as f32 - zy;
                            (dx * dx + dy * dy).sqrt() < 25.0
                        })
                        .collect();
                    let prod = near.iter().filter(|b| b.produces.is_some()).count();
                    let total = near.len();
                    // Stacking check: any two buildings within 1 tile of each other
                    let mut stacked = 0;
                    for i in 0..near.len() {
                        for j in (i + 1)..near.len() {
                            let dx = near[i].grid_x as i32 - near[j].grid_x as i32;
                            let dy = near[i].grid_y as i32 - near[j].grid_y as i32;
                            if dx.abs() <= 1 && dy.abs() <= 1 {
                                stacked += 1;
                            }
                        }
                    }
                    if total < 8 || prod < 3 || stacked > 0 {
                        bad += 1;
                        println!(
                            "size {size} 1v{enemies} seed {seed} capital {zi} at ({},{}): {total} buildings, {prod} production, {stacked} stacked pairs",
                            z.center_gx, z.center_gy
                        );
                    }
                }
            }
        }
    }
    println!("done, {bad} bad capitals");
}
