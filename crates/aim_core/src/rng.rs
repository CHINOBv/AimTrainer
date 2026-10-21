//! Small deterministic PRNG (SplitMix64). Seeded sessions make scenarios reproducible and testable.

use crate::math::Range;

/// SplitMix64 generator; the same seed always yields the same sequence.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform value inside the inclusive range.
    pub fn in_range(&mut self, r: Range) -> f32 {
        r.lerp(self.next_f32())
    }

    /// Randomly returns 1.0 or -1.0.
    pub fn sign(&mut self) -> f32 {
        if self.next_u64() & 1 == 0 { 1.0 } else { -1.0 }
    }
}

/// Deals the values `0..n` in shuffled rounds: each value comes exactly once every `n`
/// draws, so random choices stay balanced without long streaks or droughts.
#[derive(Debug, Clone, PartialEq)]
pub struct ShuffleBag {
    order: Vec<u8>,
    next: usize,
}

impl ShuffleBag {
    /// A bag of the values `0..n` (`n` ≥ 1); the first draw shuffles it.
    pub fn new(n: u8) -> Self {
        Self {
            order: (0..n.max(1)).collect(),
            next: usize::from(n.max(1)),
        }
    }

    /// The next value, reshuffling (Fisher–Yates) when a round is over.
    pub fn draw(&mut self, rng: &mut Rng) -> u8 {
        if self.next >= self.order.len() {
            for i in (1..self.order.len()).rev() {
                let j = (rng.next_u64() % (i as u64 + 1)) as usize;
                self.order.swap(i, j);
            }
            self.next = 0;
        }
        self.next += 1;
        self.order[self.next - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence_and_values_stay_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let v = a.in_range(Range::new(-2.0, 3.0));
            assert_eq!(v, b.in_range(Range::new(-2.0, 3.0)));
            assert!((-2.0..=3.0).contains(&v));
        }
    }

    #[test]
    fn a_shuffle_bag_deals_every_value_once_per_round_in_a_seeded_order() {
        let deal = |seed: u64| {
            let mut rng = Rng::new(seed);
            let mut bag = ShuffleBag::new(4);
            (0..40).map(|_| bag.draw(&mut rng)).collect::<Vec<u8>>()
        };
        let drawn = deal(7);
        for round in drawn.chunks(4) {
            let mut sorted = round.to_vec();
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3]);
        }
        assert_eq!(drawn, deal(7));
        assert_ne!(drawn, deal(8));
        assert_ne!(drawn[..4], drawn[4..8], "rounds are reshuffled");
    }
}
