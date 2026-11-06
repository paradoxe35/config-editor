//! The tolerant token scan the rest of the language reads.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A string. `closed` is false while the closing quote is still missing.
    Str { closed: bool },
    /// A literal or number: `true`, `null`, `-1.5`, or an invalid bare word.
    Word,
    /// One of `{ } [ ] , :`.
    Punct(char),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) kind: Kind,
}

/// Splits `source` into tokens. Never fails: a string ends at its closing quote,
/// at a newline (JSON strings cannot span lines), or at the end of the input.
pub(super) fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some(&(start, character)) = chars.peek() {
        if character.is_whitespace() {
            chars.next();
        } else if character == '"' {
            chars.next();
            let mut end = source.len();
            let mut closed = false;
            let mut escaped = false;
            for (index, next) in chars.by_ref() {
                if next == '\n' {
                    end = index;
                    break;
                }
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == '"' {
                    end = index + next.len_utf8();
                    closed = true;
                    break;
                }
            }
            tokens.push(Token {
                start,
                end,
                kind: Kind::Str { closed },
            });
        } else if matches!(character, '{' | '}' | '[' | ']' | ',' | ':') {
            chars.next();
            tokens.push(Token {
                start,
                end: start + character.len_utf8(),
                kind: Kind::Punct(character),
            });
        } else {
            let mut end = start;
            while let Some(&(index, next)) = chars.peek() {
                if next.is_whitespace() || matches!(next, '"' | '{' | '}' | '[' | ']' | ',' | ':') {
                    break;
                }
                end = index + next.len_utf8();
                chars.next();
            }
            tokens.push(Token {
                start,
                end,
                kind: Kind::Word,
            });
        }
    }
    tokens
}

/// The text of a string token without its quotes, unescaped when closed.
pub(super) fn decode_string(text: &str, closed: bool) -> String {
    if closed {
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text.trim_matches('"').to_string())
    } else {
        text.strip_prefix('"').unwrap_or(text).to_string()
    }
}
