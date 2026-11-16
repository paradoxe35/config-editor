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
    let (read, _) = super::lexical::read(before, before.len(), |at| {
        match before.as_bytes()[at] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        false
    });
    if depth > 0 && read.quote.is_none() && read.comment.is_none() && closers.contains('}') {
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
    let read = super::lexical::read_line(line, line.len());
    (read.quote, read.comment.is_some())
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
                    // Empty when the closing line is the body's only line
                    // (the quotes, then `text` and the closing quotes): no
                    // line is wholly inside, and the closing line still
                    // belongs to the string.
                    bodies.push(first..index);
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

/// For each of `lines`, whether it is text of a multi-line string (after
/// the line opening it, through the line closing it): no header or key is
/// written there (`[alpha]` moved into a `"""` string is text).
pub(super) fn in_string_lines(lines: &[&str]) -> Vec<bool> {
    let mut inside = Vec::with_capacity(lines.len());
    let mut open: Option<&'static str> = None;
    for line in lines {
        match open {
            Some(delimiter) => {
                inside.push(true);
                if let Some(at) = closing_delimiter(line, delimiter) {
                    open = opens_unclosed_string(&line[at + 3..]);
                }
            }
            None => {
                inside.push(false);
                open = opens_unclosed_string(line);
            }
        }
    }
    inside
}

/// The delimiter of a triple-quoted string `code` opens and leaves open.
fn opens_unclosed_string(code: &str) -> Option<&'static str> {
    super::lexical::read_line(code, code.len()).multi_line
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
/// sub-tables it owns; of the table at `pointer` (`/s`, `[s]`) the same
/// way, the sub-tables right after it (`[s.sub]`) with it.
pub(super) fn table_lines(source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
    let headers = table_headers(source);
    let (start, line_end, segments) = headers.0.get(&pointer_segments(pointer))?;
    Some(*start..table_end(source, *line_end, segments, true))
}

/// The first header of each table `source` writes, by its path: where its
/// line starts and ends, and its segments. Read once while a batch runs
/// for the text (a save placing every item asks for each).
struct Headers(std::collections::HashMap<Vec<String>, (usize, usize, Vec<String>)>);

fn table_headers(source: &str) -> std::rc::Rc<Headers> {
    crate::editor::paths::batch_kept(source, || {
        crate::editor::work_count::note_many(
            crate::editor::work_count::Work::ItemLinesBytes,
            source.len(),
        );
        let lines: Vec<&str> = source.split('\n').collect();
        let paths = table_paths(&lines);
        let mut headers = std::collections::HashMap::new();
        let mut start = 0;
        for (index, line) in lines.iter().enumerate() {
            let line_end = start + line.len();
            if let Some((segments, _)) = parse_header(split_toml_comment(line).0) {
                headers
                    .entry(path_strings(&paths[index + 1]))
                    .or_insert((start, line_end, segments));
            }
            start = line_end + 1;
        }
        Headers(headers)
    })
}
