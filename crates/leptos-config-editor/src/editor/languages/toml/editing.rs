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
        // Includes any whitespace before the comment.
        let (scalar, trailing) = split_toml_comment(&code[scalar_start..]);
        let scalar_end = scalar_start + scalar.len();
        tokens.push(code_start + start, code_start + end, TokenKind::Key);
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
