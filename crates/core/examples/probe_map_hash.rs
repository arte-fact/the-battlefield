use battlefield_core::mapgen::generate_battlefield_n;

fn main() {
    for &(seed, size, caps) in &[
        (42u32, 160u32, 2u32),
        (77, 160, 2),
        (777, 224, 4),
        (123, 224, 3),
        (9999, 384, 4),
    ] {
        let (grid, layout) = generate_battlefield_n(seed, size, caps);
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        for y in 0..grid.height {
            for x in 0..grid.width {
                mix(grid.get(x, y) as u64);
                mix(grid.elevation(x, y) as u64);
                mix(format!("{:?}", grid.decoration(x, y)).len() as u64);
            }
        }
        for &(x, y) in &layout.zone_centers {
            mix(x as u64);
            mix(y as u64);
        }
        for c in &layout.connections {
            for &n in c {
                mix(n as u64);
            }
        }
        for s in &layout.settlements {
            mix(s.houses.len() as u64);
            mix(s.production.len() as u64);
            mix(s.resources.len() as u64);
        }
        println!("{seed} {size} {caps} -> {h:016x}");
    }
}
