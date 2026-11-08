//! Which key the pointer is over.

use super::*;

/// Where hover reads the context for the key at `column`: just past its `:`,
/// so a key inside a flow mapping resolves under that mapping.
pub(super) fn hover_context_offset(line: &str, column: usize) -> usize {
    let code = line_comment_start(line).map_or(line, |start| &line[..start]);
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    inline_key_at(code, b':', byte).map_or(line.len(), |key| key.after_separator)
}

pub(super) fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let code = line_comment_start(line).map_or(line, |start| &line[..start]);
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    if let Some(key) = inline_key_at(code, b':', byte) {
        let text = line[key.start..key.end].trim_matches(['\'', '"']);
        return Some((text.to_string(), line[..key.start].chars().count()));
    }
    let trimmed = line.trim_start_matches([' ', '\t']);
    let body = match trimmed.strip_prefix('-') {
        Some(rest) if rest.is_empty() || rest.starts_with([' ', '\t']) => {
            rest.trim_start_matches([' ', '\t'])
        }
        _ => trimmed,
    };
    // A comment, or a flow collection (`{n: 1}`) whose keys are inline.
    if body.starts_with(['#', '{', '[']) {
        return None;
    }
    let colon = mapping_colon_anywhere(body)?;
    let key_raw = &body[..colon];
    let key = key_raw.trim().trim_matches(['\'', '"']);
    if key.is_empty() {
        return None;
    }
    let body_start = line.len() - body.len();
    let start_column = line[..body_start].chars().count();
    let end_column = line[..body_start + key_raw.trim_end().len()]
        .chars()
        .count();
    (column >= start_column && column < end_column).then(|| (key.to_string(), start_column))
}
