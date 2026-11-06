//! The tolerant token scan the rest of the language reads.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// `<name ...`, `</name`, or `<>`: `name` is empty for a bare `<`.
    OpenTag { closing: bool, name_end: usize },
    /// `</name>` written as one token is still recorded separately so the
    /// scan can match it against the open stack.
    EndTag,
    /// Tag, attribute, or element text outside any tag.
    Text { quoted: bool },
    Equals,
    /// `<!-- ... -->` (possibly unterminated while typing).
    Comment,
    /// `<? ... ?>`, `<!DOCTYPE ...>`, `<![CDATA[ ... ]]>`.
    Special,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) kind: Kind,
}

/// Splits `source` into XML tokens. Never fails: an unclosed tag, quote, or
/// comment ends at the input end. Comments stay whole so `<!-- <a> -->` never
/// feeds markup to the scan; other `<!...>`/`<?...?>` markup is a `Special`
/// the scan ignores.
pub(super) fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < source.len() {
        let rest = &source[index..];
        let character = rest.chars().next().unwrap_or('\0');
        if character == '<' {
            let tag_start = index;
            if let Some(token) = tokenize_markup(source, bytes, tag_start) {
                index = token.end;
                tokens.push(token);
            } else {
                // A bare `<` at the very end opens a tag that has no name yet.
                tokens.push(Token {
                    start: tag_start,
                    end: source.len(),
                    kind: Kind::OpenTag {
                        closing: false,
                        name_end: source.len(),
                    },
                });
                index = source.len();
            }
        } else if character.is_whitespace() {
            index += character.len_utf8();
        } else {
            let start = index;
            while index < source.len() {
                let next = source[index..].chars().next().unwrap_or('\0');
                if next == '<' || next.is_whitespace() {
                    break;
                }
                index += next.len_utf8();
            }
            tokens.push(Token {
                start,
                end: index,
                kind: Kind::Text { quoted: false },
            });
        }
    }
    tokens
}

/// Tokenizes one `<...>` sequence starting at `tag_start`. Returns `None` for
/// a bare `<` at the end of input.
pub(super) fn tokenize_markup(source: &str, bytes: &[u8], tag_start: usize) -> Option<Token> {
    let index = tag_start + 1;
    if bytes.get(index) == Some(&b'/') {
        return Some(end_tag_token(source, tag_start, index + 1));
    }
    if source[index..].starts_with("!--") {
        let end = source[index..]
            .find("-->")
            .map_or(source.len(), |relative| index + relative + 3);
        return Some(Token {
            start: tag_start,
            end,
            kind: Kind::Comment,
        });
    }
    if source[index..].starts_with("![CDATA[") {
        let end = source[index..]
            .find("]]>")
            .map_or(source.len(), |relative| index + relative + 3);
        return Some(Token {
            start: tag_start,
            end,
            kind: Kind::Special,
        });
    }
    if bytes.get(index) == Some(&b'?') || bytes.get(index) == Some(&b'!') {
        let end = find_markup_end(source, index + 1);
        return Some(Token {
            start: tag_start,
            end,
            kind: Kind::Special,
        });
    }
    if index >= source.len() {
        return None;
    }
    let mut name_end = index;
    while name_end < source.len() {
        let next = source[name_end..].chars().next().unwrap_or('\0');
        if next.is_whitespace() || matches!(next, '/' | '>' | '<' | '=' | '"' | '\'') {
            break;
        }
        name_end += next.len_utf8();
    }
    Some(Token {
        start: tag_start,
        end: open_tag_end(source, name_end),
        kind: Kind::OpenTag {
            closing: false,
            name_end,
        },
    })
}

/// Tokenizes `</...>` starting just after the `/`, through its `>` if any.
pub(super) fn end_tag_token(source: &str, tag_start: usize, after_slash: usize) -> Token {
    let mut name_end = after_slash;
    while name_end < source.len() {
        let next = source[name_end..].chars().next().unwrap_or('\0');
        if next.is_whitespace() || matches!(next, '/' | '>') {
            break;
        }
        name_end += next.len_utf8();
    }
    let end = find_markup_end(source, name_end);
    Token {
        start: tag_start,
        end,
        kind: Kind::EndTag,
    }
}

/// End of a `<...>` sequence: the next `>` byte, or the input end when the tag
/// is still being typed.
pub(super) fn find_markup_end(source: &str, from: usize) -> usize {
    source[from..]
        .find('>')
        .map_or(source.len(), |relative| from + relative + 1)
}

/// The byte after a start tag's `>`. A tag still being typed (`<t` then a
/// new line with `<tls>`) ends before the next `<`, which can only start
/// the next tag, so the elements after it keep their places.
pub(super) fn open_tag_end(source: &str, from: usize) -> usize {
    match source[from..].find(['>', '<']) {
        Some(relative) if source.as_bytes()[from + relative] == b'>' => from + relative + 1,
        Some(relative) => from + relative,
        None => source.len(),
    }
}

/// Splits the inside of an open tag (`<name ...`) into attribute tokens:
/// names and quoted values are `Text`, the `=` between them is `Equals`.
/// Quotes may be unterminated mid-typing; a quote then runs to the tag end.
pub(super) fn tokenize_tag_inside(source: &str, open: &Token) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = match open.kind {
        Kind::OpenTag { name_end, .. } => name_end,
        _ => open.start,
    };
    while index < open.end {
        let character = source[index..].chars().next().unwrap_or('\0');
        if character.is_whitespace() || character == '/' {
            index += character.len_utf8();
        } else if character == '=' {
            tokens.push(Token {
                start: index,
                end: index + 1,
                kind: Kind::Equals,
            });
            index += 1;
        } else if character == '"' || character == '\'' {
            let start = index;
            let mut end = source.len().min(open.end);
            let mut cursor = index + 1;
            while cursor < open.end {
                if source[cursor..].starts_with(character) {
                    end = cursor + 1;
                    break;
                }
                cursor += source[cursor..].chars().next().map_or(1, char::len_utf8);
            }
            tokens.push(Token {
                start,
                end,
                kind: Kind::Text { quoted: true },
            });
            index = end;
        } else {
            let start = index;
            while index < open.end {
                let next = source[index..].chars().next().unwrap_or('\0');
                if next.is_whitespace() || matches!(next, '=' | '/' | '"' | '\'') {
                    break;
                }
                index += next.len_utf8();
            }
            tokens.push(Token {
                start,
                end: index,
                kind: Kind::Text { quoted: false },
            });
        }
    }
    tokens
}

/// The local name of the tag opened or closed at `token` (`<a:b>` yields
/// `b`). Attribute tokens and comments yield an empty name.
pub(super) fn token_name(source: &str, token: &Token) -> String {
    match token.kind {
        Kind::OpenTag { name_end, .. } => {
            let mut name = source[token.start + 1..name_end].to_string();
            if let Some(colon) = name.rfind(':') {
                name = name[colon + 1..].to_string();
            }
            name
        }
        Kind::EndTag => {
            let mut name = source[token.start + 2..]
                .split(['>', '/', '"', '\'', '=', '\t', '\n', '\r', ' '])
                .next()
                .unwrap_or("")
                .to_string();
            if let Some(colon) = name.rfind(':') {
                name = name[colon + 1..].to_string();
            }
            name
        }
        _ => String::new(),
    }
}

/// Decodes one attribute value or text fragment: entities (`&lt;`, `&#65;`)
/// resolve, unknown entities stay literal so completion keeps working.
pub(super) fn decode_entities(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(semi) = after.find(';') else {
            decoded.push_str(&rest[start..]);
            break;
        };
        let entity = &after[..semi];
        if let Some(character) = named_entity(entity) {
            decoded.push(character);
        } else if let Some(character) = numeric_entity(entity) {
            decoded.push(character);
        } else {
            decoded.push_str(&rest[start..start + 1 + semi + 1]);
        }
        rest = &after[semi + 1..];
    }
    decoded.push_str(rest);
    decoded
}

pub(super) fn named_entity(entity: &str) -> Option<char> {
    match entity {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => None,
    }
}

pub(super) fn numeric_entity(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = if let Some(hex) = digits
        .strip_prefix('x')
        .or_else(|| digits.strip_prefix('X'))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    char::from_u32(code)
}
