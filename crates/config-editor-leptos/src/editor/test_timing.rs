//! Timing in tests: the fastest of several runs.
//!
//! Tests run in parallel threads, often next to other builds, and load only
//! ever makes a run slower: the fastest of a few runs is what the code costs,
//! so a budget checked against it fails on slow code, not on a busy machine.

use std::time::{Duration, Instant};

/// Runs `work` `runs` times and returns the fastest run's duration and the
/// last run's result.
#[allow(
    dead_code,
    reason = "the timing tests that use it need languages a build may leave out"
)]
pub(crate) fn best_of<T>(runs: usize, mut work: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..runs.max(1) {
        let started = Instant::now();
        let result = work();
        best = best.min(started.elapsed());
        last = Some(result);
    }
    (best, last.expect("at least one run"))
}
