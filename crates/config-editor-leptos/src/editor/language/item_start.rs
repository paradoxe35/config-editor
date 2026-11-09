//! Where a list item opens, for the hover card: the default of
//! [`ConfigLanguage::item_start`](super::ConfigLanguage::item_start).

use super::ConfigLanguage;

/// The UTF-16 cursor at byte `offset` of the line starting at byte
/// `line_start` of `source`.
pub(crate) fn cursor_at(source: &str, line_start: usize, offset: usize) -> usize {
    source[..line_start + offset].encode_utf16().count()
}

/// When character `column` of `line` is one of `language`'s opening
/// brackets and stands where a list item goes (a JSON item's `{`), the byte
/// offset just past it; None elsewhere.
pub fn bracket_item_start<L: ConfigLanguage + ?Sized>(
    language: &L,
    source: &str,
    line_start: usize,
    line: &str,
    column: usize,
) -> Option<usize> {
    let (at, opener) = line.char_indices().nth(column)?;
    if !language
        .config()
        .brackets
        .iter()
        .any(|(open, _)| *open == opener)
    {
        return None;
    }
    language
        .completion_context(source, cursor_at(source, line_start, at))
        .is_array_item
        .then_some(at + opener.len_utf8())
}
