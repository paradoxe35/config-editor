//! The seeded random source and how many cases a run draws.

/// A small fast generator (SplitMix64): the same seed draws the same cases
/// on every machine.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..bound` (`0` when `bound` is `0`).
    pub(super) fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next() % bound as u64) as usize
    }

    /// True `percent` times in a hundred.
    pub(super) fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub(super) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// An index into `weights`, each drawn as often as its weight says.
    pub(super) fn weighted(&mut self, weights: &[usize]) -> usize {
        let total: usize = weights.iter().sum();
        let mut roll = self.below(total);
        for (index, weight) in weights.iter().enumerate() {
            if roll < *weight {
                return index;
            }
            roll -= weight;
        }
        weights.len() - 1
    }
}

/// How many cases a property runs: its own modest count, or
/// `SCHEMA_FUZZ_CASES` when set.
pub(super) fn cases(default: usize) -> usize {
    std::env::var("SCHEMA_FUZZ_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

/// The seed of case `index` of the property named `name`: from
/// `SCHEMA_FUZZ_SEED` when set.
pub(super) fn seed(name: &str, index: usize) -> u64 {
    let base: u64 = std::env::var("SCHEMA_FUZZ_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(38);
    let named = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    base.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ named ^ (index as u64).wrapping_mul(0x9e37_79b9)
}
