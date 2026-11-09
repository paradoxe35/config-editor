//! Which key the pointer is over.

use super::*;

/// On an explicit key's line (`? k`) past its `?`, on its value's line
/// (`: v`) right below its key's lines, or on a line of the block that
/// value holds: the path there, the key and the column it starts at
/// (0-based). `None` on any other line (the completion context reads it).
pub(super) fn explicit_key_path(source: &str, line_index: usize, column: usize) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let lines: Vec<&str> = source.split('\n').collect();
    let (start, text) = entry_body(lines.get(line_index)?);
    let entries = || yaml_path_lines(source).into_iter();
    let (path, key_column) = if text.starts_with("? ") {
        if column < lines[line_index][..start].chars().count() {
            return None;
        }
        let (path, _, key_column, _) = entries().rfind(|(_, line, _, _)| *line == line_index + 1)?;
        (path, key_column)
    } else if text == ":" || text.starts_with(": ") {
        let key_line = explicit_key_above(&lines, line_index, start)?;
        let (path, _, key_column, _) = entries().rfind(|(_, line, _, _)| *line == key_line + 1)?;
        (path, key_column)
    } else if !text.is_empty() && !text.starts_with('#') && under_explicit_value(&lines, line_index, start) {
        let (path, _, key_column, _) = entries().rfind(|(_, line, at, _)| *line == line_index + 1 && *at <= column + 1)?;
        (path, key_column)
    } else {
        return None;
    };
    let key = path.last()?.clone();
    let path = path
        .into_iter()
        .map(SchemaPathSegment::from_pointer_segment)
        .collect();
    Some((path, key, key_column - 1))
}

/// The line of the explicit key (`? k`) whose `: value` line is
/// `line_index`, its body at byte `start`: the entry right above at that
/// column.
fn explicit_key_above(lines: &[&str], line_index: usize, start: usize) -> Option<usize> {
    let above = (0..line_index).rev().find(|&above| {
        let (at, text) = entry_body(lines[above]);
        !text.is_empty() && !text.starts_with('#') && at <= start
    })?;
    let (at, text) = entry_body(lines[above]);
    (at == start && text.starts_with("? ")).then_some(above)
}

/// Whether line `line_index` (its body at byte `start`) is inside the block
/// an explicit key's `:` line holds, at any depth.
fn under_explicit_value(lines: &[&str], line_index: usize, start: usize) -> bool {
    let mut inside = start;
    for above in (0..line_index).rev() {
        let line = lines[above];
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        let (at, text) = entry_body(line);
        if text.is_empty() || text.starts_with('#') || indent >= inside {
            continue;
        }
        if text == ":" || text.starts_with(": ") {
            return explicit_key_above(lines, above, at).is_some();
        }
        if indent == 0 {
            return false;
        }
        inside = indent;
    }
    false
}

/// The text of `line` after its indentation and item dashes, and the byte
/// it starts at.
fn entry_body(line: &str) -> (usize, &str) {
    let mut body = line.trim_start_matches([' ', '\t']);
    while let Some(rest) = body.strip_prefix("- ") {
        body = rest.trim_start_matches([' ', '\t']);
    }
    (line.len() - body.len(), body)
}

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
    // A key's own anchor or tag (`&k b: 1`) is no part of it.
    let properties = colon - without_node_properties(&body[..colon]).len();
    let (body, colon) = (&body[properties..], colon - properties);
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
