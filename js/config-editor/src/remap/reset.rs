//! A host's ranges carried into a new text of the host's own (a reset, a
//! file loaded again): `replaceValue`, or a new `value` given without
//! ranges. That text is no edit of the old one: its changed stretches tell
//! nothing of where the user's characters went, so growing a range over
//! one (as [`super::Remap`] does for an echo of the typing) would lock
//! every line a newer version of the file changed after the lock (QA 38,
//! H38-3).
//!
//! The two texts' lines are diffed, and each locked line goes to the line
//! holding its text in the new one: the line the diff kept it as; else, in
//! its changed run, the line holding the same text, or the line aligned
//! with it there when its own text changed in place (the line sharing the
//! longest start with it, else the one at its place); else the same text
//! nearest to where it would be (a line moved further down or up); else the
//! aligned line. No range is dropped: one whose line is gone lands on the
//! line now in its place.

use std::collections::HashMap;
use std::ops::Range;

use config_editor_leptos::{TextPosition, TextRange};

use super::stretches::line_runs::line_runs;

/// How far from its place in a changed run a changed line looks for the
/// line it became (lines each way): the work per range stays bounded.
const ALIGN_WINDOW: usize = 64;

#[cfg(test)]
thread_local! {
    /// New lines compared while aligning (the tests count work).
    pub(crate) static ALIGNED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What the diff made of an old line.
#[derive(Clone, Copy)]
enum Diffed {
    /// Kept, as this new line.
    Kept(usize),
    /// Changed, in this run (an index into `runs`).
    Changed(usize),
}

/// Where an old line goes: a new line, and whether it holds the same text.
#[derive(Clone, Copy)]
struct Placed {
    at: usize,
    same: bool,
}

/// Places ranges of `old`'s lines in `new`'s (both as the editor reads
/// them: see `super::as_read`).
pub(crate) struct Carry<'a> {
    old: Vec<&'a str>,
    new: Vec<&'a str>,
    diffed: Vec<Diffed>,
    runs: Vec<(Range<usize>, Range<usize>)>,
    /// The new lines in changed runs, by text and by trimmed text, each
    /// list in order.
    added: HashMap<&'a str, Vec<usize>>,
    added_trimmed: HashMap<&'a str, Vec<usize>>,
}

impl<'a> Carry<'a> {
    pub(crate) fn new(old: &'a str, new: &'a str) -> Self {
        let old: Vec<&str> = old.split('\n').collect();
        let new: Vec<&str> = new.split('\n').collect();
        let runs = line_runs(&old, &new);
        let mut diffed = Vec::with_capacity(old.len());
        let mut added: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut added_trimmed: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut next_new = 0;
        for (index, (old_run, new_run)) in runs.iter().enumerate() {
            // The lines before the run are kept, one for one.
            while diffed.len() < old_run.start {
                diffed.push(Diffed::Kept(next_new));
                next_new += 1;
            }
            diffed.extend(old_run.clone().map(|_| Diffed::Changed(index)));
            for at in new_run.clone() {
                added.entry(new[at]).or_default().push(at);
                let trimmed = new[at].trim();
                if !trimmed.is_empty() {
                    added_trimmed.entry(trimmed).or_default().push(at);
                }
            }
            next_new = new_run.end;
        }
        while diffed.len() < old.len() {
            diffed.push(Diffed::Kept(next_new.min(new.len() - 1)));
            next_new += 1;
        }
        Self {
            old,
            new,
            diffed,
            runs,
            added,
            added_trimmed,
        }
    }

    /// Where old line `line` (zero-based) goes.
    fn line(&self, line: usize) -> Placed {
        let run = match self.diffed[line] {
            Diffed::Kept(at) => return Placed { at, same: true },
            Diffed::Changed(run) => run,
        };
        let (old_run, new_run) = &self.runs[run];
        let text = self.old[line];
        let last = self.new.len() - 1;
        // Its place in the run: as far into it as the line is into its own.
        let place = if new_run.is_empty() {
            new_run.start.min(last)
        } else {
            (new_run.start + (line - old_run.start)).min(new_run.end - 1)
        };
        let same = self.added.get(text);
        if let Some(at) = same.and_then(|found| nearest(found, new_run, place)) {
            return Placed { at, same: true };
        }
        let aligned = (!new_run.is_empty()).then(|| self.aligned(text, new_run, place));
        if let Some(at) = aligned.filter(|at| alike(text, self.new[*at])) {
            return Placed { at, same: false };
        }
        if let Some(at) = same.and_then(|found| nearest(found, &(0..self.new.len()), place)) {
            return Placed { at, same: true };
        }
        let trimmed = text.trim();
        if let Some(at) = (!trimmed.is_empty())
            .then(|| self.added_trimmed.get(trimmed))
            .flatten()
            .and_then(|found| nearest(found, &(0..self.new.len()), place))
        {
            return Placed { at, same: false };
        }
        Placed {
            at: aligned.unwrap_or(place),
            same: false,
        }
    }

    /// The line of `run` (near `place`) aligned with the old line `text`:
    /// the one sharing the longest start with it, the nearest of those;
    /// `place` when none shares any.
    fn aligned(&self, text: &str, run: &Range<usize>, place: usize) -> usize {
        let from = place.saturating_sub(ALIGN_WINDOW).max(run.start);
        let to = (place + ALIGN_WINDOW + 1).min(run.end);
        let mut best = (0, place);
        #[cfg(test)]
        ALIGNED.with(|count| count.set(count.get() + to.saturating_sub(from)));
        for at in from..to {
            let shared = text
                .chars()
                .zip(self.new[at].chars())
                .take_while(|(left, right)| left == right)
                .count();
            if shared > best.0
                || (shared == best.0 && shared > 0 && at.abs_diff(place) < best.1.abs_diff(place))
            {
                best = (shared, at);
            }
        }
        best.1
    }

    /// Column `column` of old line `line`, on the line it went to.
    fn column(&self, line: usize, placed: Placed, column: usize) -> usize {
        if placed.same {
            return column;
        }
        let (old, new) = (self.old[line], self.new[placed.at]);
        let width = |text: &str| text.chars().count();
        let indent = |text: &str| text.chars().take_while(|c| c.is_whitespace()).count();
        // Past its text: past the new line's (a lock to the line's end).
        if column > width(old) {
            return width(new) + 1;
        }
        let (old_indent, new_indent) = (indent(old), indent(new));
        if column == old_indent + 1 {
            return new_indent + 1;
        }
        if column <= old_indent {
            return column.min(new_indent + 1);
        }
        (column - old_indent + new_indent).min(width(new) + 1)
    }

    /// `range` of the old text, in the new.
    pub(crate) fn range(&self, range: TextRange) -> TextRange {
        let last_old = self.old.len();
        let clamp = |line: usize| line.clamp(1, last_old) - 1;
        let start_line = clamp(range.start.line);
        let start_placed = self.line(start_line);
        let start = TextPosition::new(
            start_placed.at + 1,
            self.column(start_line, start_placed, range.start.column),
        );
        if range.end <= range.start {
            return TextRange::new(start, start);
        }
        // An end at the start of a line ends the line before, its break
        // included: that line is carried, not the next one.
        let whole_lines = range.end.column <= 1 && range.end.line > range.start.line;
        let end_line = clamp(if whole_lines {
            range.end.line - 1
        } else {
            range.end.line
        });
        let mut end_placed = if end_line == start_line {
            start_placed
        } else {
            self.line(end_line)
        };
        // Its last line went above its first (lines reordered): as many
        // lines as it had, from where its first went.
        if end_placed.at < start_placed.at {
            end_placed = Placed {
                at: (start_placed.at + (end_line - start_line)).min(self.new.len() - 1),
                same: false,
            };
        }
        let end = if whole_lines {
            TextPosition::new(end_placed.at + 2, 1)
        } else {
            TextPosition::new(
                end_placed.at + 1,
                self.column(end_line, end_placed, range.end.column),
            )
        };
        TextRange::new(start, end.max(start))
    }
}

/// The entry of `found` (in order) inside `within` nearest to `place`.
fn nearest(found: &[usize], within: &Range<usize>, place: usize) -> Option<usize> {
    let from = found.partition_point(|at| *at < within.start);
    let to = found.partition_point(|at| *at < within.end);
    let found = &found[from..to];
    let after = found.partition_point(|at| *at < place);
    let below = found.get(after).copied();
    let above = after.checked_sub(1).map(|before| found[before]);
    match (above, below) {
        (Some(above), Some(below)) => Some(if place - above <= below - place {
            above
        } else {
            below
        }),
        (one, other) => one.or(other),
    }
}

/// Whether line `new` reads as `old` changed in place: the text they share
/// at their start and end is at least half of each.
fn alike(old: &str, new: &str) -> bool {
    let head = old
        .chars()
        .zip(new.chars())
        .take_while(|(left, right)| left == right)
        .count();
    let (old_rest, new_rest) = (
        old.chars().skip(head).collect::<Vec<_>>(),
        new.chars().skip(head).collect::<Vec<_>>(),
    );
    let tail = old_rest
        .iter()
        .rev()
        .zip(new_rest.iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let shared = head + tail;
    let (old_width, new_width) = (head + old_rest.len(), head + new_rest.len());
    shared > 0 && 2 * shared >= old_width && 2 * shared >= new_width
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
