use super::keyboard::utf16_to_byte_index;
use super::language::ConfigLanguage;

pub(super) fn line_at_cursor(source: &str, cursor: usize) -> usize {
    source[..utf16_to_byte_index(source, cursor)]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

/// For copy or cut with nothing selected: the UTF-16 range of the caret's
/// whole line, its newline included, as VS Code copies it.
pub(super) fn line_range(source: &str, cursor: usize) -> (usize, usize) {
    let byte = utf16_to_byte_index(source, cursor);
    let start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let end = source[byte..]
        .find('\n')
        .map_or(source.len(), |index| byte + index + 1);
    (
        source[..start].encode_utf16().count(),
        source[..end].encode_utf16().count(),
    )
}

/// What Tab inserts at `cursor` (UTF-16) with no multi-line selection:
/// spaces up to the next tab stop (`abc|` → `abc |` with 2-space indents),
/// or a tab in tab-indented documents.
pub(super) fn tab_insert(source: &str, cursor: usize, unit: &str) -> String {
    let unit = unit.to_string();
    if unit == "\t" {
        return unit;
    }
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..byte].chars().count();
    " ".repeat(unit.len() - column % unit.len())
}

/// Where Home moves the caret at `cursor` (UTF-16): the line's first
/// non-blank character, or the line start when already there.
pub(super) fn smart_home(source: &str, cursor: usize) -> usize {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..];
    let line = &line[..line.find('\n').unwrap_or(line.len())];
    let first = line_start + (line.len() - line.trim_start_matches([' ', '\t']).len());
    let target = if byte == first { line_start } else { first };
    source[..target].encode_utf16().count()
}

/// For F8: the index of the first of the sorted `offsets` strictly after
/// (or before) `here`, wrapping at the ends.
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
/// UTF-16 offset of one-based (`line`, `column`), clamped to the line and
/// the document.
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

/// Lines scanned by `indent_unit`, so large documents are not scanned on
/// every keystroke.
pub(super) const INDENT_SCAN_LINE_LIMIT: usize = 400;

/// Widest indent, in spaces, taken as a unit; wider ones are alignment.
pub(super) const MAX_INDENT_WIDTH: usize = 8;

/// Indent width, in spaces, when no indented line is found.
pub(super) const DEFAULT_INDENT_WIDTH: usize = 2;

/// The document's indentation unit (a tab, or its narrowest space indent).
pub fn indent_unit(source: &str) -> String {
    let mut widths = Vec::new();
    for line in source.lines().take(INDENT_SCAN_LINE_LIMIT) {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let leading = &line[..line.len() - trimmed.len()];
        // A line of only indentation (one being typed) says nothing yet.
        if leading.is_empty() || trimmed.is_empty() {
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
    unit: &str,
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

    let unit = unit.to_string();
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
                    // Back to the previous tab stop: 5 spaces become 4.
                    let spaces = line[..indent]
                        .bytes()
                        .take_while(|byte| *byte == b' ')
                        .count();
                    match spaces % unit.len() {
                        0 => spaces.min(unit.len()),
                        partial => partial,
                    }
                };
                edits.push((offset, remove, 0));
                updated.push_str(&line[remove..]);
            } else if body.trim().is_empty() {
                // Indenting a blank line would only leave trailing whitespace.
                updated.push_str(line);
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
    // As in VS Code, a selection starting at a line's start keeps covering
    // the new indent; one starting in the line moves with its text, and one
    // inside the indentation keeps its place in it (Tab then Shift+Tab gives
    // it back).
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let at_line_start = start == line_start;
    let in_indent = !at_line_start
        && source[line_start..start]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'));
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |at| start + at);
    // Edits come in line order: the start line's, if it has one, is first.
    let start_line_edit = edits
        .first()
        .copied()
        .filter(|(at, _, _)| (line_start..=line_end).contains(at));
    let updated_start = match start_line_edit {
        Some((_, removed, inserted)) if in_indent && start != end => {
            let column = start - line_start;
            let moved = (column + inserted).saturating_sub(removed);
            map_offset(line_start, false) + moved.min(column + inserted)
        }
        _ => map_offset(start, start == end || !at_line_start),
    }
    .min(updated.len());
    let updated_end = map_offset(end, true).min(updated.len());
    let updated_selection_start = updated[..updated_start].encode_utf16().count();
    let updated_selection_end = updated[..updated_end].encode_utf16().count();
    Some((updated, updated_selection_start, updated_selection_end))
}

/// Backspace in leading spaces deletes back to the previous indent stop.
/// Returns the text and caret, or `None` for a plain Backspace.
pub(super) fn backspace_indent(source: &str, cursor: usize, unit: &str) -> Option<(String, usize)> {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let before = &source[line_start..byte];
    if before.is_empty() || !before.bytes().all(|byte| byte == b' ') {
        return None;
    }
    let unit = unit.to_string();
    if unit == "\t" || unit.len() < 2 {
        return None;
    }
    let remove = match before.len() % unit.len() {
        0 => unit.len(),
        partial => partial,
    };
    let mut updated = String::with_capacity(source.len());
    updated.push_str(&source[..byte - remove]);
    updated.push_str(&source[byte..]);
    Some((updated, cursor - remove))
}

/// Typing `}`, `]`, or `)` on a blank line re-indents it to match the opening
/// bracket's line. Returns the text (closer included) and caret, or `None`
/// when typing it plainly is already right.
pub(super) fn typed_closer(
    source: &str,
    cursor: usize,
    closer: char,
    behavior: &dyn ConfigLanguage,
) -> Option<(String, usize)> {
    let opener = match closer {
        '}' => '{',
        ']' => '[',
        ')' => '(',
        _ => return None,
    };
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let before = &source[line_start..byte];
    if !before.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
        return None;
    }
    // At column 0 only on a blank line: typing `}` before text is not closing.
    let line_end = source[byte..]
        .find('\n')
        .map_or(source.len(), |offset| byte + offset);
    if before.is_empty() && !source[byte..line_end].trim().is_empty() {
        return None;
    }
    // Find the innermost unmatched opener above the caret's line, ignoring
    // brackets in strings and comments.
    let mut depth = 0usize;
    let mut open_at = None;
    'lines: for (line_offset, line) in line_offsets(&source[..line_start]).into_iter().rev() {
        let code = &line[..behavior.comment_start(line).unwrap_or(line.len())];
        for (index, character) in brackets_outside_strings(code).into_iter().rev() {
            if character == closer {
                depth += 1;
            } else if character == opener {
                if depth == 0 {
                    open_at = Some(line_offset + index);
                    break 'lines;
                }
                depth -= 1;
            }
        }
    }
    let open_line_start = source[..open_at?].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[open_line_start..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    if indent == before {
        return None;
    }
    let mut updated = String::with_capacity(source.len() + 1);
    updated.push_str(&source[..line_start]);
    updated.push_str(&indent);
    updated.push(closer);
    updated.push_str(&source[byte..]);
    let caret = source[..line_start].encode_utf16().count() + indent.encode_utf16().count() + 1;
    Some((updated, caret))
}

#[cfg(test)]
mod tab_tests {
    use super::{indent_lines, tab_insert};

    #[test]
    fn tab_goes_to_the_next_tab_stop() {
        assert_eq!(tab_insert("a:\n  b: 1\nabc", 13, "  "), " ");
        assert_eq!(tab_insert("a:\n  b: 1\n ", 11, "  "), " ");
        assert_eq!(tab_insert("a:\n  b: 1\n", 10, "  "), "  ");
    }

    #[test]
    fn shift_tab_goes_back_to_the_previous_tab_stop() {
        let source = "a:\n  b: 1\n     c: 2\n";
        let (updated, _, _) = indent_lines(source, 12, 12, true, "  ").unwrap();
        assert_eq!(updated, "a:\n  b: 1\n    c: 2\n");
    }
}

#[cfg(test)]
mod tests {
    use super::super::language::{ConfigLanguage, LanguageConfig, LanguageInfo, ParseError};
    use super::{backspace_indent, indent_lines, typed_closer};

    /// A language with `#` comments, for the bracket scan.
    struct Hash;

    impl ConfigLanguage for Hash {
        fn info(&self) -> LanguageInfo {
            LanguageInfo {
                label: "Hash",
                icon: "H",
                extensions: &[],
            }
        }
        fn config(&self) -> LanguageConfig {
            LanguageConfig {
                quotes: &["\""],
                line_comments: &["#"],
                block_comment: None,
                ..LanguageConfig::default()
            }
        }
        fn parse(&self, _source: &str) -> Result<serde_json::Value, ParseError> {
            Ok(serde_json::Value::Null)
        }
    }

    /// Applies `edit` at the `|` and returns the text with `|` at the new caret.
    fn at(marked: &str, edit: impl Fn(&str, usize) -> Option<(String, usize)>) -> Option<String> {
        let at = marked.find('|').unwrap();
        let source = marked.replace('|', "");
        let (updated, caret) = edit(&source, source[..at].encode_utf16().count())?;
        Some(format!("{}|{}", &updated[..caret], &updated[caret..]))
    }

    #[test]
    fn backspace_in_indentation_removes_one_indent_unit() {
        assert_eq!(
            at("a:\n  b: 1\n    |", |source, cursor| backspace_indent(
                source, cursor, "  "
            ))
            .as_deref(),
            Some("a:\n  b: 1\n  |")
        );
        // Off a stop, it returns to the previous stop.
        assert_eq!(
            at("a:\n  b: 1\n   |", |source, cursor| backspace_indent(
                source, cursor, "  "
            ))
            .as_deref(),
            Some("a:\n  b: 1\n  |")
        );
        // After content it is a plain Backspace.
        assert_eq!(
            at("a:\n  b|", |source, cursor| backspace_indent(
                source, cursor, "  "
            )),
            None
        );
        assert_eq!(
            at("|a", |source, cursor| backspace_indent(
                source, cursor, "  "
            )),
            None
        );
    }

    #[test]
    fn a_typed_closer_ignores_brackets_in_strings_and_comments() {
        let closer = |marked: &str, typed: char| {
            at(marked, |source, cursor| {
                typed_closer(source, cursor, typed, &Hash)
            })
        };
        assert_eq!(
            closer("ports: [\n  \"[\",\n  |", ']').as_deref(),
            Some("ports: [\n  \"[\",\n]|")
        );
        assert_eq!(
            closer("a {\n  # TODO: {\n  b = 1\n  |", '}').as_deref(),
            Some("a {\n  # TODO: {\n  b = 1\n}|")
        );
    }

    #[test]
    fn a_closer_typed_at_column_0_of_a_blank_line_aligns() {
        assert_eq!(
            at("{\n  \"a\": {\n    \"b\": 1\n|", |source, cursor| {
                typed_closer(source, cursor, '}', &Hash)
            })
            .as_deref(),
            Some("{\n  \"a\": {\n    \"b\": 1\n  }|")
        );
        // Before text it is just a character.
        assert_eq!(
            at("{\n|x", |source, cursor| typed_closer(
                source, cursor, '}', &Hash
            )),
            None
        );
    }

    #[test]
    fn a_typed_closer_on_a_blank_line_aligns_with_its_opener() {
        assert_eq!(
            at("{\n  \"a\": 1,\n  |", |source, cursor| typed_closer(
                source, cursor, '}', &Hash
            ))
            .as_deref(),
            Some("{\n  \"a\": 1,\n}|")
        );
        assert_eq!(
            at("x = {\n  a = [\n    1,\n    |", |source, cursor| {
                typed_closer(source, cursor, ']', &Hash)
            })
            .as_deref(),
            Some("x = {\n  a = [\n    1,\n  ]|")
        );
        // A closer after content is typed as is.
        assert_eq!(
            at("{\n  \"a\": 1|", |source, cursor| typed_closer(
                source, cursor, '}', &Hash
            )),
            None
        );
    }

    #[test]
    fn indenting_a_selection_leaves_blank_lines_blank() {
        let (updated, _, _) = indent_lines("a:\n\n  b: 1\n", 0, 9, false, "  ").unwrap();
        assert_eq!(updated, "  a:\n\n    b: 1\n");
    }
}

/// Each line of `text` with its byte offset.
fn line_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let at = offset;
            offset += line.len();
            (at, line.trim_end_matches('\n'))
        })
        .collect()
}

/// The brackets of `code` outside `"…"` and `'…'` strings, with their byte
/// offsets.
fn brackets_outside_strings(code: &str) -> Vec<(usize, char)> {
    let mut found = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in code.char_indices() {
        match quote {
            Some(_) if escaped => escaped = false,
            Some(_) if character == '\\' => escaped = true,
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if matches!(character, '{' | '}' | '[' | ']' | '(' | ')') => {
                found.push((index, character));
            }
            None => {}
        }
    }
    found
}
