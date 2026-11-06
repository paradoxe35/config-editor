//! Which key the pointer is over.

use super::*;

/// The mapping key under `column` (a character index) on a single line: the
/// attribute name before `=`. Block header words yield `None`: hover context
/// reads values, and headers open no value. Comment-only lines yield `None`.
pub(super) fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let code = code_before_comment(line)?;
    let byte = byte_at_column(line, column);
    if let Some(key) = inline_key_at(code, b'=', byte) {
        let raw = line[key.start..key.end].trim_matches('"');
        return (!raw.is_empty()).then(|| (raw.to_string(), line[..key.start].chars().count()));
    }
    // A block header (`letsencrypt {`, `services "web" {`): its type word or
    // a label.
    let header = code.trim_end().strip_suffix('{')?;
    let mut start = 0;
    for word in header.split_whitespace() {
        let at = start + header[start..].find(word)?;
        start = at + word.len();
        if (at..start).contains(&byte) {
            let name = word.trim_matches('"');
            return (!name.is_empty() && !name.contains('='))
                .then(|| (name.to_string(), line[..at + usize::from(word.starts_with('"'))].chars().count()));
        }
    }
    None
}

/// The byte offset of character `column` (0-based) in `line`.
pub(super) fn byte_at_column(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index)
}
