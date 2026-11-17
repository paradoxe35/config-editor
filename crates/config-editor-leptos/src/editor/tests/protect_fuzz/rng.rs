//! Seeded randomness and shrinking for the random protect runs: a run
//! is a list of actions drawn from a seed, so a failure always
//! reproduces; a failing list is shrunk by dropping actions while the
//! same kind of failure stays.

/// A small seeded generator (xorshift64*): the same seed, the same run.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    pub(super) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A number below `n` (`n` > 0).
    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub(super) fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    pub(super) fn small(&mut self) -> u16 {
        (self.next() & 0xffff) as u16
    }
}

/// A number from the environment variable `name`, else `default` (more
/// cases or steps locally: `PROTECT_FUZZ_CASES=5000 cargo test …`).
pub(super) fn env_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// `actions` with as many actions dropped as `fails` allows: chunks
/// first, then one at a time, until no single action can go.
pub(super) fn shrink<A: Clone>(actions: Vec<A>, fails: impl Fn(&[A]) -> bool) -> Vec<A> {
    let mut actions = actions;
    let mut chunk = actions.len() / 2;
    while chunk >= 1 {
        let mut start = 0;
        while start < actions.len() {
            let end = (start + chunk).min(actions.len());
            let mut fewer = actions.clone();
            fewer.drain(start..end);
            if fails(&fewer) {
                actions = fewer;
            } else {
                start += chunk;
            }
        }
        chunk /= 2;
    }
    actions
}
