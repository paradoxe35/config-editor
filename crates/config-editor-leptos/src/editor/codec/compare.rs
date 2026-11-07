//! Lines and values compared however they are written: the structural
//! patch matches the serializer's lines to the host's by these.

use super::super::language::ConfigLanguage;

/// A line as the structural patch compares it: trimmed, without quotes and
/// without its trailing comment (as the language finds it).
pub(super) fn comparable(language: &dyn ConfigLanguage, line: &str) -> String {
    let code = line[..comment_start(language, line)].trim();
    // Written forms the language reads the same (its own rules).
    let code = language.comparable_line(code);
    // A character however it is escaped (`\u00e9` for `é`, `\/` for `/`).
    let code = without_unicode_escapes(&code);
    let code = code.as_ref();
    // A number however it is written (`0x10`, `1.5e1`, `1_000`) is the same
    // line as the serializer's `16`, `15.0`, `1000`.
    let numbered;
    let code = match [": ", " = "].iter().find_map(|separator| {
        code.split_once(separator)
            .map(|(key, value)| (key, *separator, value))
    }) {
        Some((key, separator, value)) => match canonical_number(value.trim_end_matches(',').trim())
        {
            Some(number) => {
                numbered = format!(
                    "{key}{separator}{number}{}",
                    if value.trim_end().ends_with(',') {
                        ","
                    } else {
                        ""
                    }
                );
                numbered.as_str()
            }
            None => code,
        },
        None => code,
    };
    let unquoted = code.replace(['"', '\''], "");
    // A line of quotes alone (a multi-line string's `"""`) still is a line.
    if unquoted.is_empty() {
        code.to_string()
    } else {
        unquoted
    }
}

/// `code` with its `\uXXXX` escapes (a surrogate pair as one) and `\/` read.
fn without_unicode_escapes(code: &str) -> std::borrow::Cow<'_, str> {
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

/// `text` as one spelling of the number it writes, or `None` when it writes
/// none (`16` for `0x10` and `16`, `15` for `1.5e1` and `15.0`).
pub(super) fn canonical_number(text: &str) -> Option<String> {
    let text = text.replace('_', "");
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(&text)),
    };
    let radix = [("0x", 16), ("0o", 8), ("0b", 2)]
        .iter()
        .find_map(|(prefix, radix)| digits.strip_prefix(prefix).map(|rest| (rest, *radix)));
    let value = match radix {
        Some((rest, radix)) => i64::from_str_radix(rest, radix).ok()? as f64,
        None if !digits.is_empty()
            && digits.chars().all(|character| {
                character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E' | '+' | '-')
            })
            && digits
                .starts_with(|character: char| character.is_ascii_digit() || character == '.') =>
        {
            digits.parse::<f64>().ok()?
        }
        None => return None,
    } * sign;
    Some(if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    })
}

/// The trailing comment of `line`, or `""`.
pub(super) fn trailing_comment<'a>(language: &dyn ConfigLanguage, line: &'a str) -> &'a str {
    line[comment_start(language, line)..].trim_end()
}

/// Where the trailing comment of `line` starts, or its length.
pub(super) fn comment_start(language: &dyn ConfigLanguage, line: &str) -> usize {
    language
        .comment_start(line)
        .filter(|start| *start <= line.len() && line.is_char_boundary(*start))
        .unwrap_or(line.len())
}
