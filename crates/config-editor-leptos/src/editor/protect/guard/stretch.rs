//! Whether text was glued onto a span's ends: a character beside it that
//! would read as part of it.

use std::ops::Range;

/// Whether the span `was` of `old`, now at `now` of `new`, has text glued
/// onto an end: a character beside it changed to one that joins it (see
/// [`neighbour_ok`]).
pub(super) fn glued(
    old: &[u16],
    new: &[u16],
    was: &Range<usize>,
    now: &Range<usize>,
    strict: bool,
) -> bool {
    if was.start >= was.end || was.end > old.len() {
        return false;
    }
    let before_old = was.start.checked_sub(1).and_then(|at| old.get(at));
    let before_new = now.start.checked_sub(1).and_then(|at| new.get(at));
    !neighbour_ok(old[was.start], before_old, before_new, strict)
        || !neighbour_ok(old[was.end - 1], old.get(was.end), new.get(now.end), strict)
}

/// A character next to a locked stretch (whose character on that side is
/// `edge`) may change only to one that cannot join it: whitespace,
/// punctuation (only whitespace when `strict`), or nothing. Next to a
/// stretch ending in a blank or punctuation, anything may go.
fn neighbour_ok(edge: u16, old: Option<&u16>, new: Option<&u16>, strict: bool) -> bool {
    old == new
        || !glues(edge)
        || new.is_none_or(|unit| !glues(*unit) && (is_blank(*unit) || !strict))
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
