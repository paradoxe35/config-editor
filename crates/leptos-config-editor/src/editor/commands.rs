use super::keyboard::utf16_to_byte_index;

pub(super) fn line_at_cursor(source: &str, cursor: usize) -> usize {
    source[..utf16_to_byte_index(source, cursor)]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

/// Picks the next caret offset to jump to when cycling diagnostics (F8),
/// wrapping at the ends like VS Code. `offsets` are caret offsets in document
/// order; the return is a position into that slice. Equal offsets stay put so
/// repeated presses never stall on the current error.
pub(super) fn cycle_offset(offsets: &[usize], here: usize, backward: bool) -> Option<usize> {
    if offsets.is_empty() {
        return None;
    }
    if backward {
        offsets
            .iter()
            .rposition(|offset| *offset < here)
            .or(Some(offsets.len() - 1))
    } else {
        offsets.iter().position(|offset| *offset > here).or(Some(0))
    }
}
/// Returns the UTF-16 offset of the (`line`, `column`) cell, both 1-based.
/// Positions past the end of a line or document clamp to what exists, so
/// gutter clicks and squiggle clicks never produce an out-of-range caret.
pub(super) fn offset_at(source: &str, line: usize, column: usize) -> usize {
    let line_index = line.saturating_sub(1);
    let mut offset = source
        .split('\n')
        .take(line_index)
        .map(|text_line| text_line.encode_utf16().count() + 1)
        .sum::<usize>();
    offset += source
        .split('\n')
        .nth(line_index)
        .unwrap_or("")
        .chars()
        .take(column.saturating_sub(1))
        .map(char::len_utf16)
        .sum::<usize>();
    offset.min(source.encode_utf16().count())
}

/// Lines scanned when guessing the document's indent unit. Bounded so a
/// megabyte-sized document does not pay a full scan on every keystroke; the
/// unit is almost always visible in the first screenful of lines.
pub(super) const INDENT_SCAN_LINE_LIMIT: usize = 400;

/// Widest indent, in spaces, accepted as a deliberate unit. Filters out
/// alignment padding and deeply nested one-off lines so the minimum-width
/// heuristic below settles on the real step.
pub(super) const MAX_INDENT_WIDTH: usize = 8;

/// Fallback indent width, in spaces, when no indented line is found.
pub(super) const DEFAULT_INDENT_WIDTH: usize = 2;

/// Returns the indentation unit used by the document. YAML files in the wild
/// use either two or four spaces, and occasionally tabs; honouring the
/// document's own style keeps automatic indentation and expanded completions
/// consistent with the surrounding text.
pub(super) fn indent_unit(source: &str) -> String {
    let mut widths = Vec::new();
    for line in source.lines().take(INDENT_SCAN_LINE_LIMIT) {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let leading = &line[..line.len() - trimmed.len()];
        if leading.is_empty() {
            continue;
        }
        if leading.contains('\t') {
            return "\t".to_string();
        }
        widths.push(leading.len());
    }
    let width = widths
        .into_iter()
        .filter(|width| (1..=MAX_INDENT_WIDTH).contains(width))
        .min()
        .unwrap_or(DEFAULT_INDENT_WIDTH);
    " ".repeat(width)
}

/// Finds the start of a YAML comment on a single line, ignoring `#` characters
/// that appear inside quoted scalars.
pub(super) fn line_comment_start(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, character) {
            (Some('"'), '\\') => escaped = true,
            (Some(active), character) if active == character => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, '#')
                if index == 0
                    || line[..index]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_whitespace) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

/// Finds the mapping colon on a single line, ignoring colons inside quoted
/// scalars and comments.
pub(super) fn mapping_colon(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_none() && character == '#' {
            let starts_comment = index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace);
            if starts_comment {
                return None;
            }
        }
        match (quote, character) {
            (Some('"'), '\\') => escaped = true,
            (Some(active), character) if active == character => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, ':')
                if line[index + 1..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

fn code_before_comment(line: &str) -> &str {
    line_comment_start(line).map_or(line, |comment| &line[..comment])
}

fn is_block_scalar(value: &str) -> bool {
    let mut characters = value.trim().chars();
    if !matches!(characters.next(), Some('|' | '>')) {
        return false;
    }
    characters.all(|character| character.is_ascii_digit() || matches!(character, '+' | '-'))
}

pub(super) fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte_index = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte_index]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line = &source[line_start..byte_index];
    let indent: String = line
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let content = line.trim_start();
    let unit = indent_unit(source);

    if content.is_empty() {
        return format!("\n{indent}");
    }
    let code = code_before_comment(content).trim_end();
    if code.is_empty() {
        return format!("\n{indent}");
    }
    if code == "-" {
        return format!("\n{indent}- ");
    }
    if let Some(rest) = code.strip_prefix('-')
        && (rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        let after_dash = rest.trim_end();
        if after_dash.trim().is_empty() {
            return format!("\n{indent}- ");
        }
        // Preserve the document's spacing between the dash and the item's
        // content so nested lines align exactly with the key column.
        let content_offset =
            1 + after_dash.len() - after_dash.trim_start_matches([' ', '\t']).len();
        let item_indent = format!(" {}", &code[1..content_offset]);
        let content = &code[content_offset..];
        if let Some(colon) = mapping_colon(content) {
            let value = content[colon + 1..].trim();
            if value.is_empty() || is_block_scalar(value) {
                return format!("\n{indent}{item_indent}{unit}");
            }
            // An inline mapping inside a sequence item: keep the next key in
            // the same item, aligned with the current key.
            return format!("\n{indent}{item_indent}");
        }
        return format!("\n{indent}- ");
    }
    if let Some(colon) = mapping_colon(code) {
        let value = code[colon + 1..].trim();
        if value.is_empty() || is_block_scalar(value) {
            return format!("\n{indent}{unit}");
        }
    }
    format!("\n{indent}")
}

pub(super) fn toggle_yaml_line_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
) -> Option<(String, usize, usize)> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    let first_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let newline_count = source[..end].bytes().filter(|byte| *byte == b'\n').count();
    let last_line =
        newline_count.saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));

    let lines: Vec<_> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        return None;
    }

    let selected_lines = &lines[first_line..=last_line.min(lines.len() - 1)];
    let uncomment = selected_lines
        .iter()
        .filter_map(|line| {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            let content = &body[indent..];
            (!content.is_empty()).then_some(content.starts_with('#'))
        })
        .all(|commented| commented);
    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut offset = 0;
    let mut updated = String::with_capacity(source.len() + lines.len() * 2);

    for (line_index, line) in lines.iter().enumerate() {
        if (first_line..=last_line).contains(&line_index) {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            let content = &body[indent..];
            if !content.is_empty() {
                if uncomment && content.starts_with('#') {
                    let removed = 1 + usize::from(content.starts_with("# "));
                    edits.push((offset + indent, removed, 0));
                    updated.push_str(&line[..indent]);
                    updated.push_str(&content[removed..]);
                    updated.push_str(&line[body.len()..]);
                } else if !uncomment {
                    edits.push((offset + indent, 0, 2));
                    updated.push_str(&line[..indent]);
                    updated.push_str("# ");
                    updated.push_str(&line[indent..]);
                } else {
                    updated.push_str(line);
                }
            } else {
                updated.push_str(line);
            }
        } else {
            updated.push_str(line);
        }
        offset += line.len();
    }

    let map_offset = |position: usize, right_biased: bool| {
        let mut delta = 0isize;
        for (edit_position, removed, inserted) in &edits {
            if position < *edit_position {
                break;
            }
            if position == *edit_position && *removed == 0 && !right_biased {
                return (*edit_position as isize + delta) as usize;
            }
            if position <= *edit_position + *removed {
                return (*edit_position as isize + delta + *inserted as isize) as usize;
            }
            delta += *inserted as isize - *removed as isize;
        }
        (position as isize + delta) as usize
    };
    let updated_start = map_offset(start, start == end).min(updated.len());
    let updated_end = map_offset(end, true).min(updated.len());
    let updated_selection_start = updated[..updated_start].encode_utf16().count();
    let updated_selection_end = updated[..updated_end].encode_utf16().count();
    Some((updated, updated_selection_start, updated_selection_end))
}

pub(super) fn indent_yaml_lines(
    source: &str,
    selection_start: usize,
    selection_end: usize,
    outdent: bool,
) -> Option<(String, usize, usize)> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    let first_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let newline_count = source[..end].bytes().filter(|byte| *byte == b'\n').count();
    let last_line =
        newline_count.saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));
    let lines: Vec<_> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        return None;
    }

    let unit = indent_unit(source);
    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut updated = String::with_capacity(source.len() + lines.len() * 2);
    let mut offset = 0;
    for (line_index, line) in lines.iter().enumerate() {
        if (first_line..=last_line.min(lines.len() - 1)).contains(&line_index) {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            if outdent {
                let remove = if line[..indent].starts_with('\t') {
                    1
                } else {
                    line[..indent]
                        .bytes()
                        .take_while(|byte| *byte == b' ')
                        .take(unit.len())
                        .count()
                };
                edits.push((offset, remove, 0));
                updated.push_str(&line[remove..]);
            } else {
                edits.push((offset + indent, 0, unit.len()));
                updated.push_str(&line[..indent]);
                updated.push_str(&unit);
                updated.push_str(&line[indent..]);
            }
        } else {
            updated.push_str(line);
        }
        offset += line.len();
    }

    let map_offset = |position: usize, right_biased: bool| {
        let mut delta = 0isize;
        for (edit_position, removed, inserted) in &edits {
            if position < *edit_position {
                break;
            }
            if position == *edit_position && *removed == 0 && !right_biased {
                return (*edit_position as isize + delta) as usize;
            }
            if position <= *edit_position + *removed {
                return (*edit_position as isize + delta + *inserted as isize) as usize;
            }
            delta += *inserted as isize - *removed as isize;
        }
        (position as isize + delta) as usize
    };
    let updated_start = map_offset(start, start == end).min(updated.len());
    let updated_end = map_offset(end, true).min(updated.len());
    let updated_selection_start = updated[..updated_start].encode_utf16().count();
    let updated_selection_end = updated[..updated_end].encode_utf16().count();
    Some((updated, updated_selection_start, updated_selection_end))
}
