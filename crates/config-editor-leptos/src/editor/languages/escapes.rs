//! A changed string written with the escapes the host used for the string it
//! replaces: a host that writes `"caf\u00e9"` gets `"cafés"` back as
//! `"caf\u00e9s"`.

/// How a language writes a character past U+FFFF as an escape.
#[derive(Clone, Copy)]
pub(super) enum Astral {
    /// Two `\uXXXX` escapes, a UTF-16 surrogate pair (JSON).
    #[cfg_attr(not(feature = "json"), allow(dead_code))]
    SurrogatePair,
    /// One `\UXXXXXXXX` escape (YAML, TOML, HCL).
    #[cfg_attr(
        not(any(feature = "yaml", feature = "toml", feature = "hcl")),
        allow(dead_code)
    )]
    Long,
}

/// `literal` (a one-line `"…"` string reading `\uXXXX` escapes) with its
/// non-ASCII characters escaped when `written` escaped one of its own.
pub(super) fn like_written(literal: String, written: &str, astral: Astral) -> String {
    let one_line = literal.len() >= 2
        && literal.starts_with('"')
        && literal.ends_with('"')
        && !literal.starts_with("\"\"\"");
    if !one_line || literal.is_ascii() || !escapes_non_ascii(written) {
        return literal;
    }
    let mut out = String::with_capacity(literal.len() + 8);
    for character in literal.chars() {
        let code = character as u32;
        match code {
            0..=0x7f => out.push(character),
            0x80..=0xffff => out.push_str(&format!("\\u{code:04x}")),
            _ => match astral {
                Astral::SurrogatePair => {
                    let mut units = [0u16; 2];
                    for unit in character.encode_utf16(&mut units) {
                        out.push_str(&format!("\\u{unit:04x}"));
                    }
                }
                Astral::Long => out.push_str(&format!("\\U{code:08x}")),
            },
        }
    }
    out
}

/// Whether `written` holds a `\uXXXX` (or `\UXXXXXXXX`) escape of a
/// non-ASCII character.
fn escapes_non_ascii(written: &str) -> bool {
    let bytes = written.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'\\' {
            let digits = match bytes[index + 1] {
                b'u' => 4,
                b'U' => 8,
                _ => {
                    index += 2;
                    continue;
                }
            };
            if let Some(hex) = written.get(index + 2..index + 2 + digits)
                && let Ok(code) = u32::from_str_radix(hex, 16)
                && code >= 0x80
            {
                return true;
            }
            index += 2;
            continue;
        }
        index += 1;
    }
    false
}

/// `code` with its `\uXXXX` escapes (a surrogate pair as one) and `\/` read.
pub(super) fn without_unicode_escapes(code: &str) -> std::borrow::Cow<'_, str> {
    if !code.contains("\\u") && !code.contains("\\/") {
        return std::borrow::Cow::Borrowed(code);
    }
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let unit = |text: &str| {
            text.get(1..5)
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        };
        match after.chars().next() {
            Some('/') => {
                out.push('/');
                rest = &after[1..];
            }
            Some('u') if unit(after).is_some() => {
                let high = unit(after).unwrap_or_default();
                let low = after
                    .get(5..)
                    .filter(|next| next.starts_with("\\u"))
                    .and_then(|next| unit(&next[1..]));
                let (character, used) = match low {
                    Some(low)
                        if (0xd800..0xdc00).contains(&high) && (0xdc00..0xe000).contains(&low) =>
                    {
                        (
                            char::from_u32(0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00)),
                            11,
                        )
                    }
                    _ => (char::from_u32(high), 5),
                };
                match character {
                    Some(character) => {
                        out.push(character);
                        rest = &after[used..];
                    }
                    None => {
                        out.push('\\');
                        rest = after;
                    }
                }
            }
            // Any other escape (`\\`, `\"`) is kept whole.
            Some(other) => {
                out.push('\\');
                out.push(other);
                rest = &after[other.len_utf8()..];
            }
            None => {
                out.push('\\');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::{Astral, like_written};

    #[test]
    fn non_ascii_is_escaped_only_where_the_host_escaped_it() {
        let literal = || "\"café 😀\"".to_string();
        assert_eq!(
            like_written(literal(), "\"caf\\u00e9\"", Astral::SurrogatePair),
            "\"caf\\u00e9 \\ud83d\\ude00\""
        );
        assert_eq!(
            like_written(literal(), "\"caf\\u00E9\"", Astral::Long),
            "\"caf\\u00e9 \\U0001f600\""
        );
        assert_eq!(like_written(literal(), "\"café\"", Astral::Long), literal());
        // An escaped control character says nothing about non-ASCII text.
        assert_eq!(
            like_written(literal(), "\"a\\u0001\"", Astral::Long),
            literal()
        );
    }
}
