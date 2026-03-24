use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use battlefield_core::game::Game;
use battlefield_core::player_input::PlayerInput;

fn bench_game_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("game_tick");

    for seed in [42u32, 123, 7777] {
        group.bench_with_input(BenchmarkId::new("tick", seed), &seed, |b, &seed| {
            let mut game = Game::new(960.0, 640.0);
            game.setup_demo_battle_with_seed(seed);
            let input = PlayerInput {
                move_x: 1.0,
                move_y: 0.5,
                ..Default::default()
            };
            b.iter(|| {
                game.tick(&input, 1.0 / 60.0);
            });
        });
    }
    group.finish();
}

fn bench_game_update(c: &mut Criterion) {
    c.bench_function("game_update_60fps", |b| {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        b.iter(|| {
            game.update(1.0 / 60.0);
        });
    });
}

fn bench_full_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_frame");
    group.sample_size(100);

    group.bench_function("tick_and_update", |b| {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        let input = PlayerInput::default();
        let dt = 1.0 / 60.0_f32;
        b.iter(|| {
            game.tick(&input, dt);
            game.update(dt as f64);
        });
    });

    group.finish();
}

fn bench_mapgen(c: &mut Criterion) {
    c.bench_function("mapgen", |b| {
        b.iter(|| {
            battlefield_core::mapgen::generate_battlefield(42);
        });
    });
}

criterion_group!(
    benches,
    bench_game_tick,
    bench_game_update,
    bench_full_frame,
    bench_mapgen
);
criterion_main!(benches);
