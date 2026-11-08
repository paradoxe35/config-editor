//! The tolerant token scan the rest of the language reads.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// `name`, `true`, `1.5`, or another bare word. Attribute names, block
    /// types, and unquoted label words all arrive as identifiers.
    Ident,
    /// A quoted string. `closed` is false while the closing quote is missing.
    Str { closed: bool },
    /// A heredoc (`<<EOT ... EOT`) consumed whole, so its body never scans as
    /// structure. Always spans at least the opener line.
    Heredoc,
    /// A `${ ... }` / `%{ ... }` template sequence outside strings, skipped as
    /// one opaque value token.
    Template,
    /// Any other character: `{ } [ ] ( ) = , .` and stray characters, which
    /// scan as their own punctuation so the walk cannot stall.
    Punct(char),
    /// A `#`, `//`, or `/* */` comment.
    Comment,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) kind: Kind,
    /// Whether a line break sits between this token and the previous one.
    pub(super) nl_before: bool,
}

/// Splits `source` into tokens. Never fails: every token consumes at least one
/// byte, and unterminated constructs end at their line end or the input end.
pub(super) fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut nl_before = true;
    let mut index = 0;
    while index < source.len() {
        let character = source[index..].chars().next().unwrap_or('\0');
        if character.is_whitespace() {
            nl_before |= character == '\n';
            index += character.len_utf8();
            continue;
        }
        let (end, kind, spans_lines) = scan_token(source, index);
        tokens.push(Token {
            start: index,
            end,
            kind,
            nl_before,
        });
        // A token that spans lines (block comment, heredoc) leaves the next
        // token with a line break before it.
        nl_before = spans_lines;
        index = end;
    }
    tokens
}

/// The token starting at `index`: its end byte, its kind, and whether it spans
/// a line break.
pub(super) fn scan_token(source: &str, index: usize) -> (usize, Kind, bool) {
    let rest = &source[index..];
    let character = rest.chars().next().unwrap_or('\0');
    if character == '#' || rest.starts_with("//") {
        return (line_end(source, index), Kind::Comment, false);
    }
    if let Some(body) = rest.strip_prefix("/*") {
        let end = body
            .find("*/")
            .map_or(source.len(), |offset| index + 2 + offset + 2);
        return (end, Kind::Comment, source[index..end].contains('\n'));
    }
    if character == '"' {
        let (end, closed) = string_end(source, index);
        return (end, Kind::Str { closed }, false);
    }
    if rest.starts_with("<<")
        && let Some(end) = heredoc_end(source, index)
    {
        return (end, Kind::Heredoc, source[index..end].contains('\n'));
    }
    if rest.starts_with("${") || rest.starts_with("%{") {
        return (template_end(source, index), Kind::Template, false);
    }
    // HCL's names are Unicode letters and digits, `_` and `-`.
    if character.is_alphabetic() || character == '_' {
        let end = run_end(source, index, |next| {
            next.is_alphanumeric() || matches!(next, '_' | '-')
        });
        return (end, Kind::Ident, false);
    }
    if character.is_ascii_digit() {
        let end = run_end(source, index, |next| {
            next.is_ascii_alphanumeric() || matches!(next, '.' | '_' | '+' | '-')
        });
        return (end, Kind::Ident, false);
    }
    (index + character.len_utf8(), Kind::Punct(character), false)
}

/// End byte of the line holding `start`, excluding the newline.
pub(super) fn line_end(source: &str, start: usize) -> usize {
    source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset)
}

/// End byte of the run of characters from `start` that satisfy `keep`.
pub(super) fn run_end(source: &str, start: usize, keep: impl Fn(char) -> bool) -> usize {
    source[start..]
        .char_indices()
        .find(|(_, character)| !keep(*character))
        .map_or(source.len(), |(offset, _)| start + offset)
}

/// End byte of the string opening at `start`, and whether its closing quote is
/// present. A string ends at its closing quote or at the line end; a
/// template sequence in it (`"${ "}" }"`, `"%{ if x }"`) is read whole, its
/// own strings included, so a quote or comment marker inside one neither
/// closes the string nor opens a comment. `$${` and `%%{` are literal.
pub(super) fn string_end(source: &str, start: usize) -> (usize, bool) {
    let line_end = line_end(source, start + 1);
    let line = &source[..line_end];
    let mut index = start + 1;
    while index < line_end {
        let rest = &line[index..];
        let character = rest.chars().next().unwrap_or('\0');
        if character == '\\' {
            index += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
        } else if character == '"' {
            return (index + 1, true);
        } else if rest.starts_with("$${") || rest.starts_with("%%{") {
            index += 3;
        } else if rest.starts_with("${") || rest.starts_with("%{") {
            index = template_end(line, index);
        } else {
            index += character.len_utf8();
        }
    }
    (line_end, false)
}

/// Byte end of the heredoc opening at `start` (`<<MARKER` / `<<-MARKER`),
/// through the terminator line. Returns `None` when the opener has no marker,
/// so the caller falls back to punctuation tokens.
pub(super) fn heredoc_end(source: &str, start: usize) -> Option<usize> {
    let mut rest = &source[start + 2..];
    if let Some(stripped) = rest.strip_prefix('-') {
        rest = stripped;
    }
    rest = rest.trim_start_matches([' ', '\t']);
    let marker_len = rest
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .map(char::len_utf8)
        .sum::<usize>();
    if marker_len == 0 {
        return None;
    }
    let marker = &rest[..marker_len];
    let opener_end = source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset + 1);
    let mut cursor = opener_end;
    while cursor <= source.len() {
        let line_end = source[cursor..]
            .find('\n')
            .map_or(source.len(), |offset| cursor + offset);
        if source[cursor..line_end].trim() == marker {
            return Some(line_end.min(source.len()) + usize::from(line_end < source.len()));
        }
        if line_end == source.len() {
            break;
        }
        cursor = line_end + 1;
    }
    Some(source.len())
}

/// Byte end of the `${ ... }` / `%{ ... }` template opening at `start`,
/// skipping nested braces and quoted strings (templates in those included).
pub(super) fn template_end(source: &str, start: usize) -> usize {
    let mut depth = 0;
    let mut index = start;
    while index < source.len() {
        let character = source[index..].chars().next().unwrap_or('\0');
        match character {
            '"' => {
                index = string_end(source, index).0;
                continue;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += character.len_utf8();
    }
    source.len()
}

/// The text of a quoted label token without its quotes, unescaped when closed.
pub(super) fn decode_string(text: &str, closed: bool) -> String {
    if closed {
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text.trim_matches('"').to_string())
    } else {
        text.strip_prefix('"').unwrap_or(text).to_string()
    }
}

pub(super) fn token_name(source: &str, token: &Token) -> Option<String> {
    match token.kind {
        Kind::Ident => Some(source[token.start..token.end].to_string()),
        Kind::Str { closed } => Some(decode_string(&source[token.start..token.end], closed)),
        _ => None,
    }
}
