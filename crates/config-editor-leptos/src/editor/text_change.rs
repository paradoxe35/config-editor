//! Where one text changed into another, read off the two texts' bytes
//! in place (every edit asks): the bytes that changed, and the range an
//! edit replaced placed by the caret it left.

pub(super) use super::paths::utf16_len;
use super::text::{TextRange, TextSize, text_size};

/// The bytes two texts share at their start, and at their end: `suffix`
/// past the shared start, `full_suffix` regardless of it (the two may
/// overlap). Each ends on a character boundary.
struct Shared {
    prefix: usize,
    suffix: usize,
    full_suffix: usize,
}

/// Bytes compared at once while the texts agree (a slice comparison, far
/// faster than byte by byte over a long text).
const BLOCK: usize = 256;

/// The bytes `old` and `new` share at their start.
fn common_prefix(old: &[u8], new: &[u8]) -> usize {
    let length = old.len().min(new.len());
    let mut at = 0;
    while at + BLOCK <= length && old[at..at + BLOCK] == new[at..at + BLOCK] {
        at += BLOCK;
    }
    at + old[at..length]
        .iter()
        .zip(&new[at..length])
        .take_while(|(a, b)| a == b)
        .count()
}

/// The bytes `old` and `new` share at their end.
fn common_suffix(old: &[u8], new: &[u8]) -> usize {
    let length = old.len().min(new.len());
    let mut shared = 0;
    while shared + BLOCK <= length
        && old[old.len() - shared - BLOCK..old.len() - shared]
            == new[new.len() - shared - BLOCK..new.len() - shared]
    {
        shared += BLOCK;
    }
    shared
        + old[..old.len() - shared]
            .iter()
            .rev()
            .zip(new[..new.len() - shared].iter().rev())
            .take_while(|(a, b)| a == b)
            .count()
}

fn shared(before: &str, after: &str) -> Shared {
    let (old, new) = (before.as_bytes(), after.as_bytes());
    let mut prefix = common_prefix(old, new);
    while !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut full_suffix = common_suffix(old, new);
    while !before.is_char_boundary(old.len() - full_suffix) {
        full_suffix -= 1;
    }
    let mut suffix = full_suffix.min(old.len() - prefix).min(new.len() - prefix);
    while !before.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }
    Shared {
        prefix,
        suffix,
        full_suffix,
    }
}

/// The bytes `before` changed in to make `after`: where the change starts,
/// and where it ends in each text.
pub(super) fn changed_bytes(before: &str, after: &str) -> (usize, usize, usize) {
    let shared = shared(before, after);
    (
        shared.prefix,
        before.len() - shared.suffix,
        after.len() - shared.suffix,
    )
}

/// The UTF-16 range of `before` that changed into `after`: the two texts
/// are the same before and after it (the tests check the shared ends by
/// it).
#[cfg(test)]
pub(super) fn changed_range(before: &str, after: &str) -> (usize, usize) {
    let shared = shared(before, after);
    let start = utf16_len(&before[..shared.prefix]);
    let end = start + utf16_len(&before[shared.prefix..before.len() - shared.suffix]);
    (start, end)
}

/// The range of `before` the edit to `after` replaced, placed by the caret
/// it left (byte `caret` of `after`): inserted text ends there, which
/// tells apart places the texts alone cannot (`aa|a` + `a`).
pub(super) fn edited_bytes(before: &str, after: &str, caret: TextSize) -> TextRange {
    let shared = shared(before, after);
    let (start, end) = (shared.prefix, before.len() - shared.suffix);
    let (old_len, new_len) = (before.len(), after.len());
    let removed = end - start;
    let changed = TextRange::new(text_size(start), text_size(end));
    let Some(inserted) = (new_len + removed).checked_sub(old_len) else {
        return changed;
    };
    let caret = usize::from(caret);
    // Inserted text ending at the caret: the texts agree before it starts
    // and after it ends.
    if let Some(from) = caret.checked_sub(inserted)
        && caret <= new_len
        && from + removed <= old_len
        && from <= start
        && new_len - caret <= shared.full_suffix
        && before.is_char_boundary(from)
    {
        return TextRange::at(text_size(from), text_size(removed));
    }
    changed
}

#[cfg(test)]
#[path = "text_change_tests.rs"]
mod tests;
