//! Which key the pointer is over.

use super::*;

/// The element or attribute name under `column` (a character index) on a
/// single line, with its starting column. Comment-only lines yield `None`.
pub(super) fn hover_name(line: &str, column: usize) -> Option<(String, usize)> {
    let tokens = tokenize(line);
    for token in &tokens {
        match token.kind {
            Kind::OpenTag { name_end, .. } => {
                let start = token.start + 1;
                if start >= name_end {
                    continue;
                }
                let start_column = line[..start].chars().count();
                let end_column = line[..name_end.min(line.len())].chars().count();
                if (start_column..end_column).contains(&column) {
                    return Some((decode_entities(&line[start..name_end]), start_column));
                }
                for inner in tokenize_tag_inside(line, token) {
                    if !matches!(inner.kind, Kind::Text { quoted: false }) {
                        continue;
                    }
                    let start_column = line[..inner.start].chars().count();
                    let end_column = line[..inner.end].chars().count();
                    if (start_column..end_column).contains(&column) {
                        return Some((line[inner.start..inner.end].to_string(), start_column));
                    }
                }
            }
            Kind::EndTag => {
                let name = token_name(line, token);
                if name.is_empty() {
                    continue;
                }
                let start = token.start + 2;
                let start_column = line[..start].chars().count();
                let end_column = start_column
                    + line[start..]
                        .chars()
                        .take_while(|character| !matches!(character, '>' | '/' | ' ' | '\t'))
                        .count();
                if (start_column..end_column).contains(&column) {
                    return Some((name, start_column));
                }
            }
            Kind::Comment => return None,
            _ => {}
        }
    }
    None
}

/// Byte offset within `line` where the hover context is read: just past the
/// first `>` after the name at `column`, so one line holding a whole subtree
/// resolves the hover under the right path.
pub(super) fn hover_context_end(line: &str, column: usize) -> usize {
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    let name_start = hover_name(line, column).map_or(byte, |(_, start_column)| {
        line.char_indices()
            .nth(start_column)
            .map_or(line.len(), |(index, _)| index)
    });
    let name_end = line[name_start..]
        .find(|character: char| matches!(character, '>' | '/' | '=') || character.is_whitespace())
        .map_or(line.len(), |relative| name_start + relative);
    // An attribute (`name="…"`) resolves inside its quotes, where its value
    // is addressed.
    let rest = &line[name_end..];
    let after_equals = rest.trim_start().strip_prefix('=').map(str::trim_start);
    match after_equals {
        Some(value) if value.starts_with(['"', '\'']) => line.len() - value.len() + 1,
        // An element whose start tag carries attributes: read just past its
        // name, where the context is the element itself.
        _ if rest.starts_with([' ', '\t']) && !rest.trim_start().starts_with(['>', '/']) => name_end + 1,
        _ => name_end,
    }
}
