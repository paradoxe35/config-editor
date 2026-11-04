use super::keyboard::utf16_to_byte_index;

pub(super) fn line_at_cursor(source: &str, cursor: usize) -> usize {
    source[..utf16_to_byte_index(source, cursor)]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

pub(super) fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte_index = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte_index]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let current_line = &source[line_start..byte_index];
    let indent: String = current_line
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let extra_indent = if current_line.trim_end().ends_with(':') {
        "  "
    } else {
        ""
    };
    format!("\n{indent}{extra_indent}")
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
                        .take(2)
                        .count()
                };
                edits.push((offset, remove, 0));
                updated.push_str(&line[remove..]);
            } else {
                edits.push((offset + indent, 0, 2));
                updated.push_str(&line[..indent]);
                updated.push_str("  ");
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
