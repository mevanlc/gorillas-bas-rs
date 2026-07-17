//! Faithful reimplementation of QuickBASIC's `RND` pseudo-random generator.
//!
//! QBasic uses a 24-bit linear congruential generator:
//! `seed = (seed * 0xFD43FD + 0xC39EC3) mod 2^24`, returning `seed / 2^24`.
//! `RANDOMIZE n` mixes `n` into the high 16 bits of the seed. The original game
//! calls `RANDOMIZE (TIMER)` before each round and defines
//! `FNRan(x) = INT(RND(1) * x) + 1`, i.e. a uniform integer in `1..=x`.

pub struct QbRng {
    seed: u32, // 24-bit state
}

const MULT: u32 = 0x00FD_43FD;
const INC: u32 = 0x00C3_9EC3;
const MOD: u32 = 0x0100_0000; // 2^24

impl QbRng {
    /// Create with QBasic's default startup seed.
    pub fn new() -> Self {
        QbRng { seed: 0x0050_0000 }
    }

    /// Equivalent to `RANDOMIZE n`: replace the high 16 bits of the 24-bit seed
    /// with `n`'s low 16 bits, keeping the low 8 bits of the current seed.
    pub fn randomize(&mut self, n: u32) {
        let hi = (n & 0xFFFF) << 8;
        self.seed = (hi | (self.seed & 0xFF)) & (MOD - 1);
    }

    /// `RND` → float in `[0, 1)`.
    pub fn rnd(&mut self) -> f64 {
        self.seed = (self.seed.wrapping_mul(MULT).wrapping_add(INC)) & (MOD - 1);
        self.seed as f64 / MOD as f64
    }

    /// `FNRan(x) = INT(RND * x) + 1` → integer in `1..=x`.
    pub fn ran(&mut self, x: i32) -> i32 {
        (self.rnd() * x as f64) as i32 + 1
    }
}

impl Default for QbRng {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rnd_in_unit_interval() {
        let mut r = QbRng::new();
        for _ in 0..10_000 {
            let v = r.rnd();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn ran_in_range() {
        let mut r = QbRng::new();
        r.randomize(12345);
        for _ in 0..10_000 {
            let v = r.ran(6);
            assert!((1..=6).contains(&v));
        }
    }

    #[test]
    fn deterministic_for_seed() {
        let mut a = QbRng::new();
        a.randomize(42);
        let mut b = QbRng::new();
        b.randomize(42);
        for _ in 0..100 {
            assert_eq!(a.rnd(), b.rnd());
        }
    }
}
