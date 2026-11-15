//! Timing in tests, for promises a work count cannot state (time spent in
//! a parser or validator of another crate).
//!
//! Tests run in parallel threads, often next to other builds, and load only
//! ever makes a run slower. So a timing test never compares a run with a
//! fixed budget: it compares two sizes of the same work measured in the
//! same test, the sizes taken in turn (so a busy stretch slows both alike),
//! each the fastest of several runs (what the code costs), against a
//! generous bound (linear work at four times the size takes about four
//! times as long, quadratic work sixteen times).

use std::time::{Duration, Instant};

/// Runs this short tell nothing of growth: a run is compared as if it
/// took at least this long.
const FLOOR: Duration = Duration::from_millis(2);

/// The time of the same work on a small and a large input.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "the timing tests that use it need languages a build may leave out"
)]
pub(crate) struct Growth {
    pub(crate) small: Duration,
    pub(crate) large: Duration,
}

#[allow(
    dead_code,
    reason = "the timing tests that use it need languages a build may leave out"
)]
impl Growth {
    /// How many times longer the large input took (each run compared as
    /// if it took at least [`FLOOR`]).
    pub(crate) fn ratio(&self) -> f64 {
        self.large.max(FLOOR).as_secs_f64() / self.small.max(FLOOR).as_secs_f64()
    }
}

impl std::fmt::Display for Growth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} then {:?} ({:.1}x)",
            self.small,
            self.large,
            self.ratio()
        )
    }
}

/// `work` timed on `small` and on `large`, taken in turn `runs` times:
/// the fastest run of each.
#[allow(
    dead_code,
    reason = "the timing tests that use it need languages a build may leave out"
)]
pub(crate) fn growth<I: ?Sized, T>(
    runs: usize,
    (small, large): (&I, &I),
    mut work: impl FnMut(&I) -> T,
) -> Growth {
    let mut time = |input: &I| {
        let started = Instant::now();
        drop(std::hint::black_box(work(input)));
        started.elapsed()
    };
    let mut best = (Duration::MAX, Duration::MAX);
    for _ in 0..runs.max(1) {
        best.0 = best.0.min(time(small));
        best.1 = best.1.min(time(large));
    }
    Growth {
        small: best.0,
        large: best.1,
    }
}
