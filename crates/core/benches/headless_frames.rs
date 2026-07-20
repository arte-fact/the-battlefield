use battlefield_core::game::Game;
use battlefield_core::player_input::PlayerInput;
use std::time::Instant;

fn main() {
    let frames: usize = std::env::var("BENCH_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600); // 60 seconds at 60fps

    let seed: u32 = std::env::var("BENCH_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    println!("=== Headless Frame Benchmark ===");
    println!("Frames: {frames}, Seed: {seed}");

    let mut game = Game::new(960.0, 640.0);
    // Optional balance-tuning overrides for run-to-end probes
    if let Ok(n) = std::env::var("BENCH_ENEMIES") {
        if let Ok(n) = n.parse::<u8>() {
            game.config.enemy_count = n.clamp(1, 3);
        }
    }
    if let Some(mp) = std::env::var("BENCH_MANPOWER")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        game.config.manpower_start = mp;
    }
    if let Some(bleed) = std::env::var("BENCH_BLEED")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        game.config.bleed_per_extra_zone = bleed;
    }
    game.setup_demo_battle_with_seed(seed);

    let input = PlayerInput {
        move_x: 0.5,
        move_y: 0.3,
        ..Default::default()
    };
    let dt = 1.0 / 60.0_f32;

    // With BENCH_RUN_TO_END=1, stop as soon as a faction wins (battle-length
    // probe for the manpower/conquest rules) instead of always running the
    // full frame count.
    let run_to_end = std::env::var("BENCH_RUN_TO_END").is_ok_and(|v| v == "1");

    let start = Instant::now();
    let mut frame_times = Vec::with_capacity(frames);
    let mut prev_pos: std::collections::HashMap<u32, (f32, f32)> = std::collections::HashMap::new();

    let dump_interval = std::env::var("BENCH_DUMP")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 1);
    for frame in 0..frames {
        let frame_start = Instant::now();
        game.tick(&input, dt);
        game.update(dt as f64);
        frame_times.push(frame_start.elapsed());
        if let Some(every) = dump_interval {
            if frame % every == 0 {
                println!("t={}s", frame as f32 * dt);
                print!("{}", game.flow_diagnostics());
                let mut moved = 0.0f32;
                let mut still = 0usize;
                for u in game.units.iter().filter(|u| u.alive) {
                    if let Some(&(px, py)) = prev_pos.get(&u.id) {
                        let d = ((u.x - px).powi(2) + (u.y - py).powi(2)).sqrt();
                        moved += d;
                        if d < 32.0 {
                            still += 1;
                        }
                    }
                }
                println!(
                    "displacement since last dump: total={:.0}px still_units={} of {}",
                    moved,
                    still,
                    game.units.iter().filter(|u| u.alive).count()
                );
                prev_pos.clear();
                for u in game.units.iter().filter(|u| u.alive) {
                    prev_pos.insert(u.id, (u.x, u.y));
                }
            }
        }
        if run_to_end && game.winner.is_some() {
            break;
        }
    }

    let total = start.elapsed();
    let frames = frame_times.len();
    let avg_us = total.as_micros() as f64 / frames as f64;

    use battlefield_core::unit::{Faction, UnitKind};
    let alive = |f| {
        game.units
            .iter()
            .filter(|u| u.alive && u.faction == f)
            .count()
    };
    let comp = |f: Faction| {
        let count = |k| {
            game.units
                .iter()
                .filter(|u| u.alive && u.faction == f && u.kind == k)
                .count()
        };
        format!(
            "W{} L{} A{} M{}",
            count(UnitKind::Warrior),
            count(UnitKind::Lancer),
            count(UnitKind::Archer),
            count(UnitKind::Monk)
        )
    };
    println!();
    let active = game.active_factions();
    let mp: Vec<String> = active
        .iter()
        .map(|f| format!("{:.1}", game.manpower[f.idx()]))
        .collect();
    let al: Vec<String> = active.iter().map(|&f| alive(f).to_string()).collect();
    println!(
        "Battle state after {:.0}s simulated: winner={:?}, manpower=[{}], alive=[{}]",
        frames as f32 * dt,
        game.winner,
        mp.join(", "),
        al.join(", "),
    );
    let comps: Vec<String> = active
        .iter()
        .map(|&f| format!("{:?}=[{}]", f, comp(f)))
        .collect();
    println!("Composition: {}", comps.join(" "));
    let zones: String = game
        .zone_manager
        .zones
        .iter()
        .map(|z| match z.state {
            battlefield_core::zone::ZoneState::Controlled(Faction::Blue) => 'B',
            battlefield_core::zone::ZoneState::Controlled(Faction::Red) => 'R',
            battlefield_core::zone::ZoneState::Capturing(Faction::Blue) => 'b',
            battlefield_core::zone::ZoneState::Capturing(Faction::Red) => 'r',
            battlefield_core::zone::ZoneState::Contested => 'c',
            battlefield_core::zone::ZoneState::Controlled(Faction::Yellow) => 'Y',
            battlefield_core::zone::ZoneState::Capturing(Faction::Yellow) => 'y',
            battlefield_core::zone::ZoneState::Controlled(Faction::Purple) => 'P',
            battlefield_core::zone::ZoneState::Capturing(Faction::Purple) => 'p',
            battlefield_core::zone::ZoneState::Controlled(Faction::Villager)
            | battlefield_core::zone::ZoneState::Capturing(Faction::Villager) => 'v',
            battlefield_core::zone::ZoneState::Neutral => 'n',
        })
        .collect();
    println!("Zones: [{zones}]");
    if std::env::var("BENCH_DUMP").is_ok() {
        println!("{}", game.flow_diagnostics());
        // ASCII map: terrain + units around each army blob
        use battlefield_core::grid::TileKind;
        for (label, fac) in [("BLUE", Faction::Blue), ("RED", Faction::Red)] {
            let cells: Vec<(u32, u32)> = game
                .units
                .iter()
                .filter(|u| u.alive && u.faction == fac)
                .map(|u| u.grid_cell())
                .collect();
            if cells.is_empty() {
                continue;
            }
            let cx = cells.iter().map(|c| c.0).sum::<u32>() / cells.len() as u32;
            let cy = cells.iter().map(|c| c.1).sum::<u32>() / cells.len() as u32;
            println!("{label} blob around ({cx},{cy}):");
            for y in cy.saturating_sub(12)..cy + 13 {
                let mut row = String::new();
                for x in cx.saturating_sub(24)..cx + 25 {
                    let n_here = game
                        .units
                        .iter()
                        .filter(|u| u.alive && u.grid_cell() == (x, y))
                        .count();
                    let ch = if n_here > 0 {
                        let f = game
                            .units
                            .iter()
                            .find(|u| u.alive && u.grid_cell() == (x, y))
                            .unwrap()
                            .faction;
                        if f == Faction::Blue {
                            'b'
                        } else {
                            'r'
                        }
                    } else if !game.grid.in_bounds(x as i32, y as i32) {
                        '#'
                    } else {
                        match game.grid.get(x, y) {
                            TileKind::Water => '~',
                            TileKind::Forest => 'T',
                            TileKind::Rock => '^',
                            TileKind::Road => '=',
                            _ => {
                                if game.grid.elevation(x, y) > 0 {
                                    'E'
                                } else if !game.grid.is_passable(x, y) {
                                    'X'
                                } else {
                                    '.'
                                }
                            }
                        }
                    };
                    row.push(ch);
                }
                println!("{row}");
            }
        }
    }

    let mut sorted_us: Vec<u128> = frame_times.iter().map(|d| d.as_micros()).collect();
    sorted_us.sort();

    let p95_us = sorted_us[sorted_us.len() * 95 / 100];
    let p99_us = sorted_us[sorted_us.len() * 99 / 100];
    let max_us = sorted_us[sorted_us.len() - 1];

    println!();
    println!("Total time: {:.2}s", total.as_secs_f64());
    println!("Avg frame:  {:.1}us ({:.2}ms)", avg_us, avg_us / 1000.0);
    println!("P95 frame:  {}us ({:.2}ms)", p95_us, p95_us as f64 / 1000.0);
    println!("P99 frame:  {}us ({:.2}ms)", p99_us, p99_us as f64 / 1000.0);
    println!("Max frame:  {}us ({:.2}ms)", max_us, max_us as f64 / 1000.0);
    println!("Effective FPS: {:.1}", 1_000_000.0 / avg_us);

    let budget_ms = 16.67;
    println!();
    if avg_us / 1000.0 > budget_ms {
        println!(
            "FAIL: Average frame exceeds 60fps budget ({:.2}ms > {budget_ms}ms)",
            avg_us / 1000.0
        );
    } else {
        println!(
            "OK: Average frame within 60fps budget ({:.2}ms < {budget_ms}ms)",
            avg_us / 1000.0
        );
    }

    println!();
    println!("Note: For Pi 4 estimates, multiply frame times by ~3-5x vs native x86.");
    println!(
        "      For QEMU user-mode, emulation overhead is ~10-30x (use for regression testing)."
    );
}
