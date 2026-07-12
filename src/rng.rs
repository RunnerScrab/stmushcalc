pub struct TurnRng(u64);

impl TurnRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    // Wyhash algorithm
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    #[inline(always)]
    fn next_uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn next_gaussian(&mut self, mode: f64, sigma: f64) -> f64 {
        let u1 = self.next_uniform().max(f64::MIN_POSITIVE);
        let u2 = self.next_uniform();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        mode + sigma * z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic_for_a_given_seed() {
        let mut a = TurnRng::new(42);
        let mut b = TurnRng::new(42);
        for _ in 0..8 {
            assert_eq!(a.next_gaussian(0.0, 0.08), b.next_gaussian(0.0, 0.08));
        }
    }

    #[test]
    fn centers_on_mode() {
        let mut rng = TurnRng::new(1);
        let n = 20_000;
        let mean: f64 = (0..n).map(|_| rng.next_gaussian(5.0, 0.08)).sum::<f64>() / n as f64;
        assert!((mean - 5.0).abs() < 0.01);
    }
}

