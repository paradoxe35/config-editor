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


pub(super) fn highlight(line: &str) -> Vec<Token> {
    let mut tokens = Tokens::new();
    let code_start = line.len() - line.trim_start().len();
    let (code, comment) = split_toml_comment(&line[code_start..]);
    let comment_start = code_start + code.len();
    let push_comment = |tokens: &mut Tokens| {
        if let Some(comment) = comment {
            tokens.push(comment_start, comment_start + comment.len(), TokenKind::Comment);
        }
    };
    if code.trim().is_empty() {
        push_comment(&mut tokens);
    } else if code.starts_with('[') {
        let header_len = code.trim_end().len();
        tokens.push(code_start, code_start + header_len, TokenKind::Punctuation);
        push_comment(&mut tokens);
    } else if let Some((_, start, end)) = key_assignment(code)
        && let Some(equals) = code[end..].find('=').map(|index| end + index)
    {
        let after_equals = &code[equals + 1..];
        let gap_len = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
        let scalar_start = equals + 1 + gap_len;
        tokens.push(code_start + start, code_start + end, TokenKind::Key);
        // An array or inline table: its elements, as on its lines after.
        if code[scalar_start..].starts_with(['[', '{']) {
            let value_start = code_start + scalar_start;
            super::constructs::value_tokens(&line[value_start..], value_start, &mut tokens);
            return tokens.finish();
        }
        // Includes any whitespace before the comment.
        let (scalar, trailing) = split_toml_comment(&code[scalar_start..]);
        let scalar_end = scalar_start + scalar.len();
        tokens.push(code_start + scalar_start, code_start + scalar_end, scalar_kind(scalar));
        if let Some(trailing) = trailing {
            tokens.push(
                code_start + scalar_end,
                code_start + scalar_end + trailing.len(),
                TokenKind::Comment,
            );
        }
        push_comment(&mut tokens);
    }
    tokens.finish()
}

pub(super) fn scalar_kind(scalar: &str) -> TokenKind {
    let scalar = scalar.trim();
    if scalar.is_empty() {
        TokenKind::Value
    } else if matches!(scalar, "true" | "false") || scalar.parse::<f64>().is_ok() {
        TokenKind::Literal
    } else if scalar.starts_with(['"', '\'', '{', '[']) {
        TokenKind::String
    } else {
        TokenKind::Value
    }
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

/// Where `delimiter` closes a string in `text` (not escaped, for `"""`).
fn closing_delimiter(text: &str, delimiter: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = text[from..].find(delimiter).map(|at| from + at) {
        let backslashes = text[..at].chars().rev().take_while(|character| *character == '\\').count();
        if delimiter.starts_with('\'') || backslashes % 2 == 0 {
            return Some(at);
        }
        from = at + 1;
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
            if is_array && next.len() > segments.len() && next.starts_with(&segments) {
                last_end = offset;
                continue;
            }
            break;
        }
        if !code.is_empty() {
            last_end = offset;
        }
    }
    (start, end.max(source[..last_end].encode_utf16().count()))
}
