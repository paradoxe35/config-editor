//! Counts of costly work, for the performance tests.
//!
//! A test that guards against work repeated per item (a read of the whole
//! document for each problem, a path lookup for each line) counts that work
//! instead of timing it: a count is the same on any machine under any load.
//! The code doing the work calls [`note`] at the one place it starts; in a
//! build without tests that is nothing at all.

/// A kind of costly work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Work {
    /// A read of a whole document for the spans of its keys or values
    /// (a language's span lookup, or one read shared by many).
    DocumentRead,
    /// The path at a place of a text: a read of the text from the top.
    PathLookup,
    /// A whole text split into its lines.
    LineSplit,
    /// A scan of a whole document recording where every key and value is
    /// written (with its path), where only the caret's place was wanted.
    #[cfg_attr(
        not(any(feature = "json", feature = "xml")),
        allow(dead_code, reason = "only the JSON and XML scanners note it")
    )]
    SpanScan,
}

impl Work {
    #[cfg(test)]
    const fn index(self) -> usize {
        match self {
            Self::DocumentRead => 0,
            Self::PathLookup => 1,
            Self::LineSplit => 2,
            Self::SpanScan => 3,
        }
    }
}

#[cfg(test)]
thread_local! {
    static COUNTS: std::cell::Cell<[usize; 4]> = const { std::cell::Cell::new([0; 4]) };
}

/// `work` is starting (counted on this thread in tests).
#[inline(always)]
pub(crate) fn note(work: Work) {
    #[cfg(test)]
    COUNTS.with(|counts| {
        let mut now = counts.get();
        now[work.index()] += 1;
        counts.set(now);
    });
    #[cfg(not(test))]
    let _ = work;
}

/// Runs `run` and returns its result and how many times it started `work`.
#[cfg(test)]
pub(crate) fn counted<T>(work: Work, run: impl FnOnce() -> T) -> (T, usize) {
    let before = COUNTS.with(std::cell::Cell::get)[work.index()];
    let result = run();
    let after = COUNTS.with(std::cell::Cell::get)[work.index()];
    (result, after - before)
}

#[cfg(test)]
mod tests {
    use super::{Work, counted, note};

    #[test]
    fn counts_only_the_work_asked_inside_the_run() {
        note(Work::PathLookup);
        let ((), reads) = counted(Work::DocumentRead, || {
            note(Work::DocumentRead);
            note(Work::PathLookup);
            note(Work::DocumentRead);
        });
        assert_eq!(reads, 2);
        let ((), lookups) = counted(Work::PathLookup, || note(Work::PathLookup));
        assert_eq!(lookups, 1);
    }
}
