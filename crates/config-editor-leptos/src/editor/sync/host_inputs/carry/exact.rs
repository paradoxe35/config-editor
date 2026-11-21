//! A host's ranges carried from one text to another by what the diff
//! knows, never by a guess (looking for the nearest copy of a
//! range's text picked the wrong one of two equal values, and left the
//! locked one editable).
//!
//! The texts' lines are diffed (`line_runs`), and inside each changed run
//! of lines its characters, twice: Myers from the start and from the end
//! (for runs up to [`CHAR_CAP`] characters and its depth cap). Where two
//! alignments are equally short they differ, and those two are its
//! extremes; a range both carry alike is known: on the characters it
//! covered (every one kept), or grown over just what replaced the part of
//! its text that changed (most of it kept). Any other range gets the safe
//! answer: the whole lines its changed runs now cover, and every place the
//! new text holds its text (moved: once in each text, that place alone). A
//! lock may grow; it never moves onto a place the diff did not lead it to
//! alone, and is never dropped.

use std::ops::Range;

use crate::editor::paths::{TextPosition, TextRange};

use super::stretches::line_runs::{Run, line_runs, myers};

/// The most characters (both texts together) a changed run of lines is
/// diffed by character in: a bigger one locks its whole lines.
const CHAR_CAP: usize = 4096;

/// The most places a range's text, gone from its run, is locked at.
const MAX_COPIES: usize = 64;

#[cfg(test)]
thread_local! {
    /// Characters diffed (the tests count work).
    pub(crate) static DIFFED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A text (as the editor reads it: LF breaks, no byte order mark) and
/// where each of its lines starts.
pub(crate) struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        Self { text, starts }
    }

    /// The byte of `position` (a column past its line's end is its end, a
    /// line past the text the text's end).
    pub(crate) fn byte(&self, position: TextPosition) -> usize {
        let Some(&start) = self.starts.get(position.line.saturating_sub(1)) else {
            return self.text.len();
        };
        let line = self.line(position.line.saturating_sub(1));
        start
            + line
                .char_indices()
                .nth(position.column.saturating_sub(1))
                .map_or(line.len(), |(at, _)| at)
    }

    fn line_of(&self, byte: usize) -> usize {
        self.starts.partition_point(|start| *start <= byte) - 1
    }

    pub(crate) fn position(&self, byte: usize) -> TextPosition {
        let byte = byte.min(self.text.len());
        let line = self.line_of(byte);
        let column = self.text[self.starts[line]..byte].chars().count() + 1;
        TextPosition::new(line + 1, column)
    }

    /// Line `line` (zero-based) without its break.
    fn line(&self, line: usize) -> &'a str {
        let start = self.starts[line];
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.text.len(), |next| next - 1);
        &self.text[start..end]
    }

    /// The byte where line `line` (zero-based) ends, its break included.
    fn line_end(&self, line: usize) -> usize {
        self.starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len())
    }
}

/// A stretch the texts differ in (bytes), in the changed run `run`;
/// `coarse` when the run was too big to diff by character (it is the run).
#[derive(Clone, Debug)]
struct Change {
    old: Range<usize>,
    new: Range<usize>,
    run: usize,
    coarse: bool,
}

/// How a range was carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Carried {
    /// On the characters it covered, every one kept.
    Exact,
    /// Its text changed in place: grown over what replaced it.
    Grown,
    /// The whole lines its runs now cover, and the lines holding its line's
    /// text where it was gone.
    Lines,
}

/// Where one alignment carries a range: its bytes in the new text, the
/// runs whose changes touch it, and how much of it they took.
#[derive(Debug, PartialEq, Eq)]
struct Placed {
    bytes: Range<usize>,
    touched: Vec<usize>,
    removed: usize,
    coarse: bool,
}

/// Carries ranges of `old` into `new`.
pub(crate) struct Exact<'a> {
    old: Lines<'a>,
    new: Lines<'a>,
    /// The changed runs of lines, as bytes of each text.
    runs: Vec<(Range<usize>, Range<usize>)>,
    /// Where they differ, in order: by the alignment from the start and by
    /// the one from the end.
    changes: [Vec<Change>; 2],
    /// Whether the new text is an edit of the old by the user (an echo's
    /// document): locked text is then all there, so a range whose text the
    /// diff saw changed was aligned wrong, and gets the safe answer.
    edit: bool,
}

impl<'a> Exact<'a> {
    /// `new` an edit of `old` by the user (`edit`), or a text of the
    /// host's own.
    pub(crate) fn new(old: &'a str, new: &'a str, edit: bool) -> Self {
        let (old_lines, new_lines) = (Lines::new(old), Lines::new(new));
        let split = |text: &'a str| -> Vec<&'a str> { text.split_inclusive('\n').collect() };
        let (old_split, new_split) = (split(old), split(new));
        let bytes = |lines: &Lines<'_>, run: Range<usize>, text: &str| {
            let at = |line: usize| lines.starts.get(line).copied().unwrap_or(text.len());
            at(run.start)..at(run.end)
        };
        // The line diff's changed runs, those only blank lines part joined
        // (blank lines align any way: a line diff splits a line from the
        // text it joined across one).
        let mut runs: Vec<(Range<usize>, Range<usize>)> = Vec::new();
        for (old_run, new_run) in line_runs(&old_split, &new_split) {
            let run = (
                bytes(&old_lines, old_run, old),
                bytes(&new_lines, new_run, new),
            );
            match runs.last_mut() {
                Some(last) if old[last.0.end..run.0.start].trim().is_empty() => {
                    last.0.end = run.0.end;
                    last.1.end = run.1.end;
                }
                _ => runs.push(run),
            }
        }
        let mut changes = [Vec::new(), Vec::new()];
        for (run, (old_run, new_run)) in runs.iter().enumerate() {
            let [forward, backward] = char_changes(old, new, old_run, new_run, run);
            changes[0].extend(forward);
            changes[1].extend(backward);
        }
        Self {
            old: old_lines,
            new: new_lines,
            runs,
            changes,
            edit,
        }
    }

    /// Where byte `offset` of the old text is in the new one by `changes`,
    /// for a range's start (`at_end` false) or end: inside a change, at
    /// its start or end; at text inserted there, before it for an end and
    /// after it for a start.
    fn offset(changes: &[Change], offset: usize, at_end: bool) -> usize {
        let passed = changes.partition_point(|change| {
            if at_end {
                offset > change.old.start && offset >= change.old.end
            } else {
                offset >= change.old.end
            }
        });
        let (old_from, new_from) = passed.checked_sub(1).map_or((0, 0), |last| {
            (changes[last].old.end, changes[last].new.end)
        });
        match changes.get(passed) {
            Some(change) if offset > change.old.start => {
                if at_end {
                    change.new.end
                } else {
                    change.new.start
                }
            }
            _ => new_from + (offset - old_from),
        }
    }

    /// Old bytes `start..end` by `changes`.
    fn place(changes: &[Change], start: usize, end: usize) -> Placed {
        let new_start = Self::offset(changes, start, false);
        let new_end = Self::offset(changes, end, true).max(new_start);
        let mut placed = Placed {
            bytes: new_start..new_end,
            touched: Vec::new(),
            removed: 0,
            coarse: false,
        };
        let from = changes.partition_point(|change| change.old.end < start);
        for change in &changes[from..] {
            if change.old.start > end || (change.old.start == end && end > start) {
                break;
            }
            let inside = if change.old.is_empty() {
                change.old.start > start && change.old.start < end
            } else {
                change.old.start < end && change.old.end > start
            };
            if inside {
                placed.removed += change.old.end.min(end) - change.old.start.max(start);
                placed.coarse |= change.coarse;
                placed.touched.push(change.run);
            }
        }
        placed
    }

    fn range(&self, bytes: Range<usize>) -> TextRange {
        TextRange::new(self.new.position(bytes.start), self.new.position(bytes.end))
    }

    /// `range` of the old text in the new (one range, or several for text
    /// that may be on several lines now), and how it was carried.
    pub(crate) fn carry(&self, range: TextRange) -> (Vec<TextRange>, Carried) {
        let (start, end) = (self.old.byte(range.start), self.old.byte(range.end));
        let end = end.max(start);
        let [forward, backward] = [0, 1].map(|at| Self::place(&self.changes[at], start, end));
        let length = end - start;
        let known = forward == backward;
        if known && forward.touched.is_empty() {
            debug_assert_eq!(
                &self.new.text[forward.bytes.clone()],
                &self.old.text[start..end],
                "an exact carry covers its old text"
            );
            return (vec![self.range(forward.bytes)], Carried::Exact);
        }
        if known
            && !self.edit
            && !forward.coarse
            && length > 0
            && 2 * (length - forward.removed) >= length
        {
            return (vec![self.range(forward.bytes)], Carried::Grown);
        }
        // Gone from its run, or not known: the whole lines its runs now
        // cover (both alignments' places included).
        let mut low = forward.bytes.start.min(backward.bytes.start);
        let mut high = forward.bytes.end.max(backward.bytes.end);
        for run in forward.touched.iter().chain(&backward.touched) {
            low = low.min(self.runs[*run].1.start);
            high = high.max(self.runs[*run].1.end);
        }
        let first = self.new.line_of(low);
        let last = self.new.line_of(high.saturating_sub(1).max(low));
        let high = self
            .new
            .line_end(last)
            .max(self.new.starts[first] + 1)
            .min(self.new.text.len());
        // At the text's end (after its last break): the line before too.
        let low = if high == self.new.starts[first] {
            self.new.starts[first.saturating_sub(1)]
        } else {
            self.new.starts[first]
        };
        let mut placed = vec![self.range(low..high)];
        // Its text, wherever the new text holds it (moved): written once in
        // each text, that place alone.
        if let Some((copies, only)) = self.copies(start, end) {
            if only {
                return (copies, Carried::Exact);
            }
            placed.extend(copies);
        }
        (placed, Carried::Lines)
    }

    /// Each place the new text holds old bytes `start..end`'s text, and
    /// whether each text holds it once (it moved there). `None` for no
    /// text, or past [`MAX_COPIES`] places (its runs' lines then).
    fn copies(&self, start: usize, end: usize) -> Option<(Vec<TextRange>, bool)> {
        let text = &self.old.text[start..end];
        if text.trim().is_empty() {
            return None;
        }
        let found: Vec<usize> = self
            .new
            .text
            .match_indices(text)
            .map(|(at, _)| at)
            .take(MAX_COPIES + 1)
            .collect();
        if found.len() > MAX_COPIES {
            return None;
        }
        let once = found.len() == 1 && self.old.text.match_indices(text).nth(1).is_none();
        let places = found
            .into_iter()
            .map(|at| self.range(at..at + text.len()))
            .collect();
        Some((places, once))
    }
}

/// The changes inside one changed run of lines (bytes of each text), by
/// the alignment from the start and the one from the end: by character
/// when it is small enough, else the whole run (`coarse`) for both.
fn char_changes(
    old: &str,
    new: &str,
    old_run: &Range<usize>,
    new_run: &Range<usize>,
    run: usize,
) -> [Vec<Change>; 2] {
    let (old_text, new_text) = (&old[old_run.clone()], &new[new_run.clone()]);
    let whole = || {
        vec![Change {
            old: old_run.clone(),
            new: new_run.clone(),
            run,
            coarse: true,
        }]
    };
    let old_chars: Vec<(usize, char)> = old_text.char_indices().collect();
    let new_chars: Vec<(usize, char)> = new_text.char_indices().collect();
    if old_chars.len() + new_chars.len() > CHAR_CAP {
        return [whole(), whole()];
    }
    #[cfg(test)]
    DIFFED.with(|diffed| diffed.set(diffed.get() + 2 * (old_chars.len() + new_chars.len())));
    let chars = |list: &[(usize, char)]| list.iter().map(|(_, c)| *c).collect::<Vec<char>>();
    let (old_only, new_only) = (chars(&old_chars), chars(&new_chars));
    let reversed = |list: &[char]| list.iter().rev().copied().collect::<Vec<char>>();
    let forward = myers(&old_only, &new_only);
    // From the end: the same diff of the reversed texts, read back.
    let backward = myers(&reversed(&old_only), &reversed(&new_only)).map(|found| {
        let flip = |part: Range<usize>, length: usize| length - part.end..length - part.start;
        let mut runs: Vec<Run> = found
            .into_iter()
            .map(|(old_part, new_part)| {
                (
                    flip(old_part, old_only.len()),
                    flip(new_part, new_only.len()),
                )
            })
            .collect();
        runs.reverse();
        runs
    });
    let byte = |chars: &[(usize, char)], text: &str, at: usize| {
        chars.get(at).map_or(text.len(), |(byte, _)| *byte)
    };
    let changes = |found: Option<Vec<Run>>| match found {
        None => whole(),
        Some(found) => found
            .into_iter()
            .map(|(old_part, new_part)| Change {
                old: old_run.start + byte(&old_chars, old_text, old_part.start)
                    ..old_run.start + byte(&old_chars, old_text, old_part.end),
                new: new_run.start + byte(&new_chars, new_text, new_part.start)
                    ..new_run.start + byte(&new_chars, new_text, new_part.end),
                run,
                coarse: false,
            })
            .collect(),
    };
    [changes(forward), changes(backward)]
}

#[cfg(test)]
#[path = "exact_tests.rs"]
mod tests;
