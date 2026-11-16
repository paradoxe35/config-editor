//! Whether text was glued onto a span's ends: a character beside it that
//! would read as part of it.

use super::super::super::text::{TextRange, TextSize, slice};

/// The character of `text` just before byte `at`, if any.
pub(super) fn char_before(text: &str, at: TextSize) -> Option<char> {
    slice::before(text, at)?.chars().next_back()
}

/// The character of `text` starting at byte `at`, if any.
pub(super) fn char_at(text: &str, at: TextSize) -> Option<char> {
    slice::after(text, at)?.chars().next()
}

/// Whether the span `was` of `old`, now at `now` of `new`, has text glued
/// onto an end: a character beside it changed to one that joins it (see
/// [`neighbour_ok`]).
pub(super) fn glued(old: &str, new: &str, was: &TextRange, now: &TextRange, strict: bool) -> bool {
    let (Some(first), Some(last)) = (char_at(old, was.start()), char_before(old, was.end())) else {
        return false;
    };
    if was.is_empty() {
        return false;
    }
    let before_old = char_before(old, was.start());
    let before_new = char_before(new, now.start());
    !neighbour_ok(first, before_old, before_new, strict)
        || !neighbour_ok(
            last,
            char_at(old, was.end()),
            char_at(new, now.end()),
            strict,
        )
}

/// A character next to a locked stretch (whose character on that side is
/// `edge`) may change only to one that cannot join it: whitespace,
/// punctuation (only whitespace when `strict`), or nothing. Next to a
/// stretch ending in a blank or punctuation, anything may go.
fn neighbour_ok(edge: char, old: Option<char>, new: Option<char>, strict: bool) -> bool {
    old == new || !glues(edge) || new.is_none_or(|new| !glues(new) && (is_blank(new) || !strict))
}

/// Whether `character` is whitespace.
pub(super) fn is_blank(character: char) -> bool {
    character.is_whitespace()
}

/// Whether a character written next to text would read as part of it.
fn glues(character: char) -> bool {
    !character.is_whitespace() && !",;:=(){}[]<>#/".contains(character)
}
