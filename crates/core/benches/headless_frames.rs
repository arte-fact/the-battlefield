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

    for _ in 0..frames {
        let frame_start = Instant::now();
        game.tick(&input, dt);
        game.update(dt as f64);
        frame_times.push(frame_start.elapsed());
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
    println!(
        "Battle state after {:.0}s simulated: winner={:?}, manpower=[{:.1}, {:.1}], alive=[{}, {}]",
        frames as f32 * dt,
        game.winner,
        game.manpower[0],
        game.manpower[1],
        alive(Faction::Blue),
        alive(Faction::Red),
    );
    println!(
        "Composition: blue=[{}] red=[{}]",
        comp(Faction::Blue),
        comp(Faction::Red)
    );
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
            battlefield_core::zone::ZoneState::Neutral => 'n',
        })
        .collect();
    println!("Zones: [{zones}]");

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
