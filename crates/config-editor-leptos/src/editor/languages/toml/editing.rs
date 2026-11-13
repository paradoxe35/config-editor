//! Enter, comments, auto-closing, lexical state, and syntax colors.

use super::*;

/// Inline tables and table headers cannot span lines, so Enter inside one
/// with only closers after the caret (`{ token = "x"| }`, `[web|]`) steps
/// past them and starts the next line there.
pub(super) fn inline_table_exit(source: &str, cursor: usize) -> Option<Newline> {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[byte..].find('\n').map_or(source.len(), |index| byte + index);
    let rest = &source[byte..line_end];
    let closers = split_toml_comment(rest).0.trim_end();
    let before = &source[line_start..byte];
    let header = before.trim_start().starts_with('[') && !before.contains('=');
    // Inside an inline table with more of it after the caret
    // (`{ a = 1,| b = 2 }`): TOML keeps inline tables on one line, so the
    // new line starts after the whole line's code.
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for character in before.chars() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '{') => depth += 1,
            (None, '}') => depth -= 1,
            _ => {}
        }
    }
    if depth > 0 && quote.is_none() && closers.contains('}') {
        let line = &source[line_start..];
        let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
        return Some(Newline {
            text: format!("{closers}\n{indent}"),
            caret: closers.len() + 1 + indent.len(),
            replaces: closers.len(),
            replaces_before: 0,
        });
    }
    if closers.is_empty()
        || !(closers.contains('}') || header)
        || !closers.chars().all(|character| matches!(character, '}' | ']' | ' ' | '\t' | ','))
        || toml_lexical_state(source, cursor).0.is_some()
    {
        return None;
    }
    let line = &source[line_start..];
    let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
    Some(Newline {
        text: format!("{closers}\n{indent}"),
        caret: closers.len() + 1 + indent.len(),
        replaces: closers.len(),
        replaces_before: 0,
    })
}

/// The unclosed string quote at the caret, or comment state: the pairing guard.
pub(super) fn toml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..byte];
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '#'
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (None, true);
        }
    }
    (quote, false)
}


/// Lines inside multi-line strings' bodies: between a line that leaves a
/// triple-quoted string open and the line that closes it.
pub(super) fn multi_line_string_bodies(source: &str) -> Vec<std::ops::Range<usize>> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut bodies = Vec::new();
    let mut open: Option<(&'static str, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        match open {
            Some((delimiter, first)) => {
                if let Some(at) = closing_delimiter(line, delimiter) {
                    if first < index {
                        bodies.push(first..index);
                    }
                    open = opens_unclosed_string(&line[at + 3..]).map(|next| (next, index + 1));
                }
            }
            None => open = opens_unclosed_string(line).map(|delimiter| (delimiter, index + 1)),
        }
    }
    if let Some((_, first)) = open
        && first < lines.len()
    {
        bodies.push(first..lines.len());
    }
    bodies
}

/// The delimiter of a triple-quoted string `code` opens and leaves open.
fn opens_unclosed_string(code: &str) -> Option<&'static str> {
    let mut quote: Option<char> = None;
    let mut skip_until = 0;
    for (index, character) in code.char_indices() {
        if index < skip_until {
            continue;
        }
        match quote {
            Some('"') if character == '\\' => skip_until = index + 2,
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '#' => return None,
            None if matches!(character, '"' | '\'') => {
                let delimiter = if character == '"' { "\"\"\"" } else { "'''" };
                if code[index..].starts_with(delimiter) {
                    match closing_delimiter(&code[index + 3..], delimiter) {
                        Some(at) => skip_until = index + 3 + at + 3,
                        None => return Some(delimiter),
                    }
                } else {
                    quote = Some(character);
                }
            }
            None => {}
        }
    }
    None
}

/// The UTF-16 selection `start..end` widened over the keys of the table
/// whose header is its last line, up to the next header (blank lines before
/// it left out).
pub(super) fn table_of_selected_header(source: &str, start: usize, end: usize) -> (usize, usize) {
    let end_byte = utf16_to_byte_index(source, end);
    let line_start = source[..end_byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[end_byte..].find('\n').map_or(source.len(), |at| end_byte + at);
    let Some((segments, is_array)) = parse_header(split_toml_comment(&source[line_start..line_end]).0) else {
        return (start, end);
    };
    let last_end = table_end(source, line_end, &segments, is_array);
    (start, end.max(crate::editor::paths::units_at(source, last_end)))
}

/// The end of the last line with code of the table whose header `segments`
/// (`is_array` for `[[…]]`) ends its line at `line_end`.
fn table_end(source: &str, line_end: usize, segments: &[String], is_array: bool) -> usize {
    let mut last_end = line_end;
    let mut offset = line_end;
    for line in source[line_end..].split('\n').skip(1) {
        let begin = offset + 1;
        offset = begin + line.len();
        let code = split_toml_comment(line).0.trim();
        if let Some((next, _)) = parse_header(code) {
            // An array-of-tables entry owns the sub-tables after it
            // (`[[s]]` then `[s.sub]`): left behind, they would join the
            // entry above.
            if is_array && next.len() > segments.len() && next.starts_with(segments) {
                last_end = offset;
                continue;
            }
            break;
        }
        if !code.is_empty() {
            last_end = offset;
        }
    }
    last_end
}

/// The bytes of the array-of-tables entry at `pointer` (`/s/1`): from its
/// `[[s]]` header through the last line with code of its table and the
/// sub-tables it owns.
pub(super) fn array_table_lines(source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
    let wanted = pointer_segments(pointer);
    let lines: Vec<&str> = source.split('\n').collect();
    let paths = table_paths(&lines);
    let mut start = 0;
    for (index, line) in lines.iter().enumerate() {
        let line_end = start + line.len();
        if let Some((segments, true)) = parse_header(split_toml_comment(line).0)
            && path_strings(&paths[index + 1]) == wanted
        {
            return Some(start..table_end(source, line_end, &segments, true));
        }
        start = line_end + 1;
    }
    None
}
