use battlefield_core::mapgen::MapGen;
use std::time::Instant;

fn main() {
    for &(size, caps) in &[(224u32, 4u32), (512, 4), (1024, 4)] {
        let mut job = MapGen::new(42, size, caps);
        let t0 = Instant::now();
        let mut steps = 0u32;
        let mut max_ms = 0.0f64;
        loop {
            let t = Instant::now();
            let done = job.step();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            if ms > max_ms {
                max_ms = ms;
            }
            steps += 1;
            if done {
                break;
            }
        }
        println!(
            "size {size} caps {caps}: {steps} steps, total {:.0} ms, max step {:.1} ms",
            t0.elapsed().as_secs_f64() * 1000.0,
            max_ms
        );
    }
}
