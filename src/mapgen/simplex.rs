/// 2D Simplex noise. Returns values in approximately [-1, 1].
/// Faster and fewer directional artifacts than classic Perlin.
pub struct Simplex {
    perm: [u8; 512],
}

// Skew/unskew factors for 2D simplex
const F2: f64 = 0.3660254037844386; // (sqrt(3) - 1) / 2
const G2: f64 = 0.21132486540518713; // (3 - sqrt(3)) / 6

// 2D gradient vectors (12 directions for better isotropy)
const GRAD2: [(f64, f64); 12] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
    (-1.0, -1.0),
    (1.0, 0.5),
    (-1.0, 0.5),
    (0.5, 1.0),
    (-0.5, 1.0),
];

impl Simplex {
    pub fn new(seed: u64) -> Self {
        let mut perm_base: [u8; 256] = [0; 256];
        for (i, slot) in perm_base.iter_mut().enumerate() {
            *slot = i as u8;
        }
        // Fisher-Yates shuffle with LCG
        let mut s = seed.wrapping_add(1);
        for i in (1..256).rev() {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (s >> 33) as usize % (i + 1);
            perm_base.swap(i, j);
        }
        let mut perm = [0u8; 512];
        for i in 0..512 {
            perm[i] = perm_base[i & 255];
        }
        Self { perm }
    }

    pub fn get(&self, x: f64, y: f64) -> f64 {
        // Skew input space to determine which simplex cell we're in
        let s = (x + y) * F2;
        let i = (x + s).floor();
        let j = (y + s).floor();

        let t = (i + j) * G2;
        let x0 = x - (i - t); // unskewed cell origin
        let y0 = y - (j - t);

        // Determine which simplex triangle we're in
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };

        let x1 = x0 - i1 as f64 + G2;
        let y1 = y0 - j1 as f64 + G2;
        let x2 = x0 - 1.0 + 2.0 * G2;
        let y2 = y0 - 1.0 + 2.0 * G2;

        let ii = (i as i32 & 255) as usize;
        let jj = (j as i32 & 255) as usize;

        // Calculate contribution from three corners
        let mut n = 0.0;

        let t0 = 0.5 - x0 * x0 - y0 * y0;
        if t0 > 0.0 {
            let t0 = t0 * t0;
            let gi = self.perm[ii + self.perm[jj] as usize] as usize % 12;
            n += t0 * t0 * (GRAD2[gi].0 * x0 + GRAD2[gi].1 * y0);
        }

        let t1 = 0.5 - x1 * x1 - y1 * y1;
        if t1 > 0.0 {
            let t1 = t1 * t1;
            let gi = self.perm[ii + i1 + self.perm[jj + j1] as usize] as usize % 12;
            n += t1 * t1 * (GRAD2[gi].0 * x1 + GRAD2[gi].1 * y1);
        }

        let t2 = 0.5 - x2 * x2 - y2 * y2;
        if t2 > 0.0 {
            let t2 = t2 * t2;
            let gi = self.perm[ii + 1 + self.perm[jj + 1] as usize] as usize % 12;
            n += t2 * t2 * (GRAD2[gi].0 * x2 + GRAD2[gi].1 * y2);
        }

        // Scale to [-1, 1]
        70.0 * n
    }

    pub fn octave(&self, x: f64, y: f64, octaves: u32, persistence: f64) -> f64 {
        let mut total = 0.0;
        let mut frequency = 1.0;
        let mut amplitude = 1.0;
        let mut max_value = 0.0;
        for _ in 0..octaves {
            total += self.get(x * frequency, y * frequency) * amplitude;
            max_value += amplitude;
            amplitude *= persistence;
            frequency *= 2.0;
        }
        total / max_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let s1 = Simplex::new(42);
        let s2 = Simplex::new(42);
        for i in 0..100 {
            let x = i as f64 * 0.1;
            let y = i as f64 * 0.07;
            assert_eq!(s1.get(x, y), s2.get(x, y));
        }
    }

    #[test]
    fn output_range() {
        let s = Simplex::new(123);
        for i in 0..1000 {
            let x = (i as f64) * 0.13 - 50.0;
            let y = (i as f64) * 0.17 - 50.0;
            let v = s.get(x, y);
            assert!((-1.5..=1.5).contains(&v), "out of range: {v} at ({x}, {y})");
        }
    }

    #[test]
    fn different_seeds_differ() {
        let s1 = Simplex::new(1);
        let s2 = Simplex::new(2);
        let mut differs = false;
        for i in 0..100 {
            let x = i as f64 * 0.1;
            let y = i as f64 * 0.07;
            if (s1.get(x, y) - s2.get(x, y)).abs() > 1e-10 {
                differs = true;
                break;
            }
        }
        assert!(differs);
    }
}
