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

/// Returns the indentation unit used by the document. Config files in the wild
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

pub(super) fn indent_lines(
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
