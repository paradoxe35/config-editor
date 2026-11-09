//! Enter, comments, auto-closing, lexical state, and syntax colors.

use super::*;

/// Start of a `#` or `//` comment in a line fragment, ignoring markers inside
/// quoted strings (and the template sequences in them: `"${ "#" }"`). A
/// marker starts a comment at the fragment start or after whitespace.
pub(super) fn line_comment_start(line: &str) -> Option<usize> {
    let mut index = 0;
    let bytes = line.as_bytes();
    while index < line.len() {
        let character = line[index..].chars().next().unwrap_or('\0');
        if let Some(end) = quoted_end(line, index) {
            index = end;
            continue;
        }
        if (character == '#' || (character == '/' && bytes.get(index + 1) == Some(&b'/')))
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

/// The end of the quoted string opening at `index` of `line` (`"…"` with
/// its template sequences, or `'…'`), or `None` when no quote opens there.
/// An unclosed one runs to the line end.
fn quoted_end(line: &str, index: usize) -> Option<usize> {
    match line.as_bytes().get(index) {
        Some(b'"') => Some(string_end(line, index).0),
        Some(b'\'') => Some(
            line[index + 1..]
                .find(['\'', '\n'])
                .map_or(line.len(), |offset| index + 1 + offset + 1),
        ),
        _ => None,
    }
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
    // So is a body line of a heredoc the text before the caret leaves
    // open (its terminator is further down).
    let line_end = source[byte..].find('\n').map_or(source.len(), |at| byte + at);
    let line = source[..byte].matches('\n').count();
    if heredoc_bodies(&source[..line_end]).iter().any(|body| body.contains(&line)) {
        return (Some('"'), false);
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
    let mut index = 0;
    while index < line.len() {
        let character = line[index..].chars().next().unwrap_or('\0');
        if in_block_comment {
            if line[index..].starts_with("*/") {
                in_block_comment = false;
                index += 2;
            } else {
                index += character.len_utf8();
            }
            continue;
        }
        if let Some(end) = quoted_end(line, index) {
            // The caret is in a string that runs to it unclosed.
            if end >= line.len() && !string_closes(line, index) {
                return (Some(character), false);
            }
            index = end;
            continue;
        }
        if (character == '#' || line[index..].starts_with("//"))
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (None, true);
        } else if line[index..].starts_with("/*") {
            in_block_comment = true;
            index += 2;
            continue;
        }
        index += character.len_utf8();
    }
    (None, in_block_comment)
}

/// Whether the string opening at `index` of `line` closes in it.
fn string_closes(line: &str, index: usize) -> bool {
    match line.as_bytes()[index] {
        b'"' => string_end(line, index).1,
        _ => line[index + 1..].contains('\''),
    }
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
    let entries = entry_keys(content, &tokens);
    let mut after_punctuation = false;
    for (index, token) in tokens.iter().enumerate() {
        let kind = if entries.contains(&index) {
            TokenKind::Key
        } else {
            let expression = after_punctuation
                || matches!(
                    tokens.get(index + 1).map(|next| next.kind),
                    Some(Kind::Punct('.' | '(' | '['))
                );
            token_kind(content, token, key_end, expression)
        };
        after_punctuation |= matches!(token.kind, Kind::Punct(_));
        if kind == TokenKind::String && matches!(token.kind, Kind::Str { .. } | Kind::Template) {
            super::interpolation::push_string(&mut out, content, indent, token.start, token.end);
            continue;
        }
        out.push(indent + token.start, indent + token.end, kind);
    }
    out.finish()
}

/// The tokens naming an attribute or an object's entry: a name or string
/// first on the line or after `{` or `,`, followed by `=` (not `==` or
/// `=>`) or `:` (`tags = { Name = "web", "kebab-key" = 1 }`).
fn entry_keys(content: &str, tokens: &[Token]) -> Vec<usize> {
    let significant: Vec<usize> = (0..tokens.len())
        .filter(|index| tokens[*index].kind != Kind::Comment)
        .collect();
    let kind = |at: Option<&usize>| at.map(|index| tokens[*index].kind);
    significant
        .iter()
        .enumerate()
        .filter(|(at, index)| {
            let token = &tokens[**index];
            let named = matches!(token.kind, Kind::Ident | Kind::Str { .. });
            let opens = *at == 0
                || matches!(
                    kind(significant.get(at - 1)),
                    Some(Kind::Punct('{' | ','))
                );
            let next = significant.get(at + 1).map(|next| &tokens[*next]);
            let separated = next.is_some_and(|next| match next.kind {
                Kind::Punct(':') => true,
                Kind::Punct('=') => !content[next.end..].starts_with(['=', '>']),
                _ => false,
            });
            named && opens && separated
        })
        .map(|(_, index)| *index)
        .collect()
}

/// The line comment markers, the one `source` writes most first (`#` when
/// it writes `//` no more often).
pub(super) fn comment_markers(source: &str) -> &'static [&'static str] {
    let (mut slashes, mut hashes) = (0usize, 0usize);
    for comment in comments(source) {
        let text = &source[comment];
        if text.starts_with("//") {
            slashes += 1;
        } else if text.starts_with('#') {
            hashes += 1;
        }
    }
    if slashes > hashes { &["//", "#"] } else { &["#", "//"] }
}

/// The `#`, `//` and `/* … */` comments (a heredoc's body is not read for
/// them).
pub(super) fn comments(source: &str) -> Vec<std::ops::Range<usize>> {
    tokenize(source)
        .iter()
        .filter(|token| token.kind == Kind::Comment)
        .map(|token| token.start..token.end)
        .collect()
}

/// The colour of `token` of `line`; `expression` says it is in an
/// expression (after punctuation, or before a `.`, `(` or `[`), where a
/// name is a reference or a function's, not a block's label.
pub(super) fn token_kind(
    line: &str,
    token: &Token,
    key_end: Option<usize>,
    expression: bool,
) -> TokenKind {
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
                } else if matches!(text, "for" | "in" | "if" | "else" | "endif" | "endfor") {
                    // The keywords of `for` expressions and template
                    // directives.
                    TokenKind::Literal
                } else if !expression {
                    // A block's bare label, or a word being typed.
                    TokenKind::Value
                } else {
                    // HCL has no bare strings: a name in an expression is a
                    // reference (`var.ami`) or a function's (`length(…)`),
                    // named like keys.
                    TokenKind::Key
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
    let code = &line[..crate::editor::language::comment_start_outside_quotes(line, &["#", "//"], &["\"", "'"]).unwrap_or(line.len())];
    let at = code.rfind("<<")?;
    if code[..at].matches('"').count() % 2 == 1 {
        return None;
    }
    let rest = code[at + 2..].strip_prefix('-').unwrap_or(&code[at + 2..]).trim();
    (!rest.is_empty() && rest.chars().all(|character| character.is_ascii_alphanumeric() || character == '_'))
        .then_some(rest)
}

/// Enter at the end of a one-line block's body (`tcp { nodelay = true| }`):
/// the block opens over lines (an HCL block's body cannot share the header's
/// line once it spans several), with the caret on a new line in it.
pub(super) fn open_one_line_block(source: &str, cursor: usize) -> Option<Newline> {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[byte..].find('\n').map_or(source.len(), |at| byte + at);
    let before = &source[line_start..byte];
    let after = &source[byte..line_end];
    // Anywhere in a block's body written on one line (`websocket {| a = 1 }`,
    // `{ a = 1 |}`): the block opens over lines, the caret on its own line
    // in it with what followed it.
    let body_after = after.trim_end().strip_suffix('}')?.trim();
    let open = before.find('{')?;
    let body_before = before[open + 1..].trim();
    if before[open + 1..].contains(['{', '}'])
        || body_after.contains(['{', '}'])
        || before[..open].contains('=')
        || body_before.is_empty() && body_after.is_empty()
    {
        return None;
    }
    let indent: String = before.chars().take_while(|character| matches!(character, ' ' | '\t')).collect();
    let unit = indent_unit(source);
    let mut opening = format!("\n{indent}{unit}");
    if !body_before.is_empty() {
        opening.push_str(body_before);
        opening.push_str(&format!("\n{indent}{unit}"));
    }
    let text = format!("{opening}{body_after}\n{indent}}}");
    Some(Newline {
        caret: opening.len(),
        text,
        replaces: after.len(),
        replaces_before: before.len() - open - 1,
    })
}
