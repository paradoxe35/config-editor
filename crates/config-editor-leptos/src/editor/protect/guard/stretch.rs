//! Whether one locked stretch survives an edit, unit by unit: its text in
//! what the two texts share, nothing glued onto its ends, its indent kept.

use std::ops::Range;

/// Whether the text from its line's start to a locked stretch (its indent)
/// reads the same after the edit.
pub(super) fn indent_kept(
    range: &Range<usize>,
    old: &[u16],
    new: &[u16],
    prefix: usize,
    suffix: usize,
) -> bool {
    let start = range.start;
    let moved = if range.end <= prefix {
        start
    } else if start >= old.len().saturating_sub(suffix) {
        start + new.len() - old.len()
    } else {
        return false;
    };
    let line_start = |units: &[u16], at: usize| {
        units[..at]
            .iter()
            .rposition(|unit| *unit == u16::from(b'\n'))
            .map_or(0, |found| found + 1)
    };
    old[line_start(old, start)..start] == new[line_start(new, moved)..moved]
}

/// Whether `range` of the old text survives the edit untouched: it lies in
/// the common prefix or suffix, and nothing new is glued onto its ends.
pub(super) fn kept(
    range: &Range<usize>,
    old: &[u16],
    new: &[u16],
    prefix: usize,
    suffix: usize,
    strict: &dyn Fn() -> bool,
) -> bool {
    if range.start >= range.end {
        return true;
    }
    let last = old[range.end - 1];
    let in_prefix =
        range.end <= prefix && neighbour_ok(last, old.get(range.end), new.get(range.end), strict);
    if in_prefix {
        return true;
    }
    let tail_start = old.len().saturating_sub(suffix);
    if range.start >= tail_start {
        let shifted = range.start + new.len() - old.len();
        let before = range.start.checked_sub(1).and_then(|at| old.get(at));
        let after = shifted.checked_sub(1).and_then(|at| new.get(at));
        return neighbour_ok(old[range.start], before, after, strict);
    }
    false
}

/// A character next to a locked stretch (whose character on that side is
/// `edge`) may change only to one that cannot join it: whitespace,
/// punctuation (only whitespace when `strict`), or nothing. Next to a
/// stretch ending in a blank or punctuation, anything may go.
fn neighbour_ok(
    edge: u16,
    old: Option<&u16>,
    new: Option<&u16>,
    strict: &dyn Fn() -> bool,
) -> bool {
    old == new
        || !glues(edge)
        || new.is_none_or(|unit| !glues(*unit) && (is_blank(*unit) || !strict()))
}

/// Whether `unit` is whitespace.
pub(super) fn is_blank(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(char::is_whitespace)
}

/// Whether a character written next to text would read as part of it.
fn glues(unit: u16) -> bool {
    match char::from_u32(u32::from(unit)) {
        Some(character) => !character.is_whitespace() && !",;:=(){}[]<>#/".contains(character),
        // Half of a surrogate pair: some symbol, part of a word.
        None => true,
    }
}
