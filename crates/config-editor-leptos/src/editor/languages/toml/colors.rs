//! Syntax colours of a TOML line: keys (bare, quoted, dotted, in inline
//! tables and table headers), values, and every literal TOML reads as a
//! number, boolean or date (`_` separators, `0x` / `0o` / `0b` integers,
//! `inf`, `nan`, dates and times).

use super::*;

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
        header_tokens(code.trim_end(), code_start, &mut tokens);
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
    } else if is_literal(scalar) {
        TokenKind::Literal
    } else if scalar.starts_with(['"', '\'', '{', '[']) {
        TokenKind::String
    } else {
        TokenKind::Value
    }
}

/// A table header's tokens (`[a."b c".d]`, `[[items]]`): its names as keys,
/// its brackets and dots as punctuation.
fn header_tokens(code: &str, offset: usize, tokens: &mut Tokens) {
    let mut index = 0;
    while index < code.len() {
        let character = code[index..].chars().next().unwrap_or('\0');
        let start = index;
        match character {
            '[' | ']' | '.' => {
                tokens.push(offset + start, offset + start + 1, TokenKind::Punctuation);
                index += 1;
            }
            '"' | '\'' => {
                index = constructs::string_end(code, index).min(code.len());
                tokens.push(offset + start, offset + index, TokenKind::Key);
            }
            _ if character.is_whitespace() => index += character.len_utf8(),
            _ => {
                let ends = |next: char| next.is_whitespace() || matches!(next, '[' | ']' | '.' | '"' | '\'');
                index = code[index..].find(ends).map_or(code.len(), |at| index + at);
                tokens.push(offset + start, offset + index, TokenKind::Key);
            }
        }
    }
}

/// Whether `scalar` is a TOML boolean, number or date-time.
fn is_literal(scalar: &str) -> bool {
    if matches!(scalar, "true" | "false") {
        return true;
    }
    let unsigned = scalar.strip_prefix(['+', '-']).unwrap_or(scalar);
    if matches!(unsigned, "inf" | "nan") {
        return true;
    }
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = scalar.strip_prefix(prefix) {
            return separated_digits(digits, radix);
        }
    }
    float(unsigned) || date_time(scalar)
}

/// Digits of `radix`, with single `_` between them.
fn separated_digits(text: &str, radix: u32) -> bool {
    !text.is_empty()
        && !text.starts_with('_')
        && !text.ends_with('_')
        && !text.contains("__")
        && text.chars().all(|digit| digit == '_' || digit.is_digit(radix))
}

/// An integer or float: `1_000`, `3.14`, `6.626e-34`.
fn float(text: &str) -> bool {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    separated_digits(whole, 10)
        && fraction.is_none_or(|fraction| separated_digits(fraction, 10))
        && exponent.is_none_or(|exponent| {
            separated_digits(exponent.strip_prefix(['+', '-']).unwrap_or(exponent), 10)
        })
}

/// A date, a time or both (`1979-05-27T07:32:00Z`, `07:32:00.5`,
/// `1979-05-27 07:32:00-07:00`).
fn date_time(text: &str) -> bool {
    let shaped = |shape: &str| {
        text.len() >= shape.len()
            && text.bytes().zip(shape.bytes()).all(|(byte, want)| {
                if want == b'9' {
                    byte.is_ascii_digit()
                } else {
                    byte == want
                }
            })
    };
    let allowed = |character: char| {
        character.is_ascii_digit() || matches!(character, '-' | ':' | '.' | '+' | 'T' | 't' | 'Z' | 'z' | ' ')
    };
    (shaped("9999-99-99") || shaped("99:99:99")) && text.chars().all(allowed)
}
