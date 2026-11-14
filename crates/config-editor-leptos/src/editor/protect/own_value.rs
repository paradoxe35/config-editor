//! The value a token is written in when its field's key is written more
//! than once (a line moved, cut and pasted, or typed into a block that
//! holds the same key: the text does not parse). The language finds one of
//! them by the field's path; which one it finds is its own affair, so a
//! token in the other one has no value written for it, and was taken as
//! plain text (typing beside it reached the host glued) or kept its whole
//! line (no way back). Each other copy is found the same way in a stand-in
//! of the text with the key found renamed (one character changed for
//! another of the same length, so every byte stays where it is): each
//! token is judged in the value it is written in.

use std::borrow::Cow;
use std::ops::Range;

use super::super::codec::scalar_range;
use super::super::language::ConfigLanguage;
use super::super::paths::{key_range, written_value_span};

/// Copies of one key a token is looked for among (a line pasted a few
/// times over); past them it has no value written for it.
const MAX_COPIES: usize = 4;

/// The bytes of the value written for the field at `pointer` that holds
/// the token at bytes `token` of `text`, and the text the language found
/// them in (`text`, or a stand-in of it where the copies of the field's
/// key before it are renamed, its bytes where they are).
pub(crate) fn own_value<'a>(
    behavior: &dyn ConfigLanguage,
    text: &'a str,
    pointer: &str,
    token: &Range<usize>,
) -> Option<(Cow<'a, str>, Range<usize>)> {
    let mut probe = Cow::Borrowed(text);
    for _ in 0..MAX_COPIES {
        let range = written_value_span(behavior, &probe, pointer)
            .and_then(|span| scalar_range(behavior, &probe, span))?;
        if range.start <= token.start && token.end <= range.end {
            return Some((probe, range));
        }
        // Another copy of the key: renamed in the stand-in.
        let key = key_range(behavior, &probe, pointer)?;
        let (start, end) = (key.start.to_offset(&probe), key.end.to_offset(&probe));
        let start = byte_of_units(&probe, start);
        let end = byte_of_units(&probe, end);
        let (at, character) = probe
            .get(start..end)?
            .char_indices()
            .find(|(_, character)| character.is_ascii_alphanumeric())?;
        let renamed = if character == 'a' { "b" } else { "a" };
        probe
            .to_mut()
            .replace_range(start + at..start + at + 1, renamed);
    }
    None
}

/// The byte of `text` at UTF-16 offset `units`.
fn byte_of_units(text: &str, units: usize) -> usize {
    super::super::keyboard::utf16_to_byte_index(text, units)
}
