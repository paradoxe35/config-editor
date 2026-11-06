//! Enter, comments, auto-closing, lexical state, and syntax colors.

use super::*;

/// Start of a `#` or `//` comment in a line fragment, ignoring markers inside
/// quoted strings. A marker starts a comment at the fragment start or after
/// whitespace.
pub(super) fn line_comment_start(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0;
    let bytes = line.as_bytes();
    while index < line.len() {
        let character = line[index..].chars().next().unwrap_or('\0');
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
        } else if (character == '#' || (character == '/' && bytes.get(index + 1) == Some(&b'/')))
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return Some(index);
        }
        index += character.len_utf8();
    }
    None
}

/// Code before a trailing `#` / `//` comment, or `None` when the fragment is
/// comment-only. Block comments are handled by the tokenizer, not here.
pub(super) fn code_before_comment(line: &str) -> Option<&str> {
    match line_comment_start(line) {
        Some(start) => {
            let code = &line[..start];
            (!code.trim().is_empty()).then_some(code)
        }
        None => (!line.trim().is_empty()).then_some(line),
    }
}

/// The unclosed string quote at the caret, or comment state: the pairing
/// guard. Heredoc bodies count as quoted so pairs never expand inside them.
pub(super) fn hcl_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let byte = utf16_to_byte_index(source, cursor);
    // Inside a heredoc body, suppress pairing without reporting a comment.
    for token in tokenize(&source[..byte]) {
        if matches!(token.kind, Kind::Heredoc) && token.start < byte && byte < token.end {
            return (Some('"'), false);
        }
    }
    // A `/*` opened before the caret without a closer keeps the caret in a
    // comment. Strings cannot span lines, so per-line quote tracking plus a
    // heredoc skip is enough.
    let mut in_block_comment = false;
    let mut skip_to = 0;
    for token in tokenize(&source[..byte]) {
        if token.start < skip_to {
            continue;
        }
        match token.kind {
            Kind::Heredoc => skip_to = token.end,
            Kind::Comment
                if source[token.start..token.end].starts_with("/*")
                    && !source[token.start..token.end].contains("*/") =>
            {
                in_block_comment = !in_block_comment;
            }
            _ => {}
        }
    }
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..byte];
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if in_block_comment {
            if line[index..].starts_with("*/") {
                in_block_comment = false;
            }
            continue;
        }
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
        } else if (character == '#' || line[index..].starts_with("//"))
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (None, true);
        } else if character == '/' && line[index..].starts_with("/*") {
            in_block_comment = true;
        }
    }
    (quote, in_block_comment)
}


pub(super) fn highlight(line: &str) -> Vec<HighlightToken> {
    let indent = line.len() - line.trim_start().len();
    let content = &line[indent..];
    let mut out = Tokens::new();
    // A comment starting the content (or the whole line) is one token.
    if let Some(comment_at) = line_comment_start(content)
        && content[..comment_at].trim().is_empty()
    {
        out.push(indent, line.len(), TokenKind::Comment);
        return out.finish();
    }
    let tokens = tokenize(content);
    // A leading identifier that opens an attribute (`key =`) or a block
    // (`type {`, `type "label" {`) is the line's key.
    let key_end = match tokens.first() {
        Some(first) if matches!(first.kind, Kind::Ident) => {
            let mut look = 1;
            while tokens
                .get(look)
                .is_some_and(|token| token.kind == Kind::Comment)
            {
                look += 1;
            }
            match tokens.get(look).map(|token| token.kind) {
                Some(Kind::Punct('=')) | Some(Kind::Punct('{')) => Some(first.end),
                Some(Kind::Ident) | Some(Kind::Str { .. })
                    if classify_header(&tokens, 0).is_some() =>
                {
                    Some(first.end)
                }
                _ => None,
            }
        }
        _ => None,
    };
    for token in &tokens {
        out.push(
            indent + token.start,
            indent + token.end,
            token_kind(content, token, key_end),
        );
    }
    out.finish()
}

pub(super) fn token_kind(line: &str, token: &Token, key_end: Option<usize>) -> TokenKind {
    match token.kind {
        Kind::Comment => TokenKind::Comment,
        Kind::Punct(_) => TokenKind::Punctuation,
        Kind::Str { .. } | Kind::Heredoc | Kind::Template => TokenKind::String,
        Kind::Ident => {
            if key_end.is_some_and(|end| token.end <= end) {
                TokenKind::Key
            } else {
                let text = &line[token.start..token.end];
                if matches!(text, "true" | "false" | "null")
                    || text
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_digit())
                {
                    TokenKind::Literal
                } else {
                    TokenKind::Value
                }
            }
        }
    }
}

/// Lines inside heredocs' bodies: between `<<MARKER` / `<<-MARKER` and the
/// line holding only the marker.
pub(super) fn heredoc_bodies(source: &str) -> Vec<std::ops::Range<usize>> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut bodies = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some(marker) = heredoc_marker(lines[index]) {
            let first = index + 1;
            let mut end = first;
            while end < lines.len() && lines[end].trim() != marker {
                end += 1;
            }
            if end > first {
                bodies.push(first..end);
            }
            index = end + 1;
        } else {
            index += 1;
        }
    }
    bodies
}

/// The marker of a heredoc opened at the end of `line`, outside strings.
fn heredoc_marker(line: &str) -> Option<&str> {
    let code = &line[..crate::editor::language::comment_start_outside_quotes(line, &["#", "//"]).unwrap_or(line.len())];
    let at = code.rfind("<<")?;
    if code[..at].matches('"').count() % 2 == 1 {
        return None;
    }
    let rest = code[at + 2..].strip_prefix('-').unwrap_or(&code[at + 2..]).trim();
    (!rest.is_empty() && rest.chars().all(|character| character.is_ascii_alphanumeric() || character == '_'))
        .then_some(rest)
}
