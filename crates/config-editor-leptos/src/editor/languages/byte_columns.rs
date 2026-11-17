//! Parsers that place an error by the byte of its line (serde_json,
//! hcl-edit), where the editor's columns count characters.

/// The 1-based character column of 1-based byte column `column` of line
/// `line` of `source` (the character holding that byte; one past the
/// line's last character past its end). The same for ASCII.
pub(super) fn char_column(source: &str, line: usize, column: usize) -> usize {
    let text = source
        .split('\n')
        .nth(line.saturating_sub(1))
        .unwrap_or_default();
    let mut byte = column.saturating_sub(1).min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].chars().count() + 1
}
