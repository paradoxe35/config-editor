//! Syntax colours of a YAML line: its key however it is written (plain,
//! quoted, with spaces or dots, non-ASCII, the merge key `<<`, a complex
//! key after `?`), its value, the directives and document markers, and
//! every literal of YAML's core schema (numbers in decimal, `0o` and `0x`
//! forms, `.inf` and `.nan`, booleans, null).

use super::*;

pub(super) fn highlight(line: &str) -> Vec<Token> {
    let mut tokens = Tokens::new();
    let indent_len = line.len() - line.trim_start().len();
    let content = &line[indent_len..];
    if content.starts_with('#') {
        tokens.push(indent_len, line.len(), TokenKind::Comment);
        return tokens.finish();
    }
    if indent_len == 0
        && let Some(mark) = document_mark(content)
    {
        tokens.push(0, mark, TokenKind::Punctuation);
        if !content[mark..].trim().is_empty() {
            highlight_value(&mut tokens, mark, &content[mark..]);
        }
        return tokens.finish();
    }
    let content_start = indent_len + item_dashes_len(content);
    let rest = &line[content_start..];
    // A complex key (`? key`) and its value (`: value`).
    for (indicator, kind) in [('?', TokenKind::Key), (':', TokenKind::Value)] {
        if let Some(after) = rest.strip_prefix(indicator)
            && after.chars().next().is_none_or(char::is_whitespace)
        {
            tokens.push(content_start, content_start + 1, TokenKind::Punctuation);
            if kind == TokenKind::Key && mapping_colon(after).is_none() {
                key_tokens(&mut tokens, content_start + 1, after);
            } else {
                highlight_rest(&mut tokens, content_start + 1, after, true);
            }
            return tokens.finish();
        }
    }
    if content_start > indent_len && mapping_colon(rest).is_none_or(|colon| !is_key(&rest[..colon])) {
        // Only a bare item dash is punctuation; one before a key stays plain.
        tokens.push(indent_len, content_start, TokenKind::Punctuation);
    }
    highlight_rest(&mut tokens, content_start, rest, content_start > indent_len);
    tokens.finish()
}

/// Tokens for `text` (at byte `offset` of the line) past any item dashes:
/// a key and its value, or (when `value` says one can stand alone there,
/// after an indicator) a value.
fn highlight_rest(tokens: &mut Tokens, offset: usize, text: &str, value: bool) {
    match mapping_colon(text).filter(|colon| is_key(&text[..*colon])) {
        Some(colon) => {
            let key = &text[..colon];
            let key_start = offset + key.len() - key.trim_start().len();
            tokens.push(key_start, offset + key.trim_end().len(), TokenKind::Key);
            tokens.push(offset + colon, offset + colon + 1, TokenKind::Punctuation);
            highlight_value(tokens, offset + colon + 1, &text[colon + 1..]);
        }
        None if value && !text.trim().is_empty() => highlight_value(tokens, offset, text),
        None => {}
    }
}

/// A complex key's text after `?` (at byte `offset`): the key, then any
/// comment.
fn key_tokens(tokens: &mut Tokens, offset: usize, text: &str) {
    let start = offset + text.len() - text.trim_start().len();
    let (key, comment) = split_yaml_comment(text.trim_start());
    let key_end = start + key.trim_end().len();
    tokens.push(start, key_end, TokenKind::Key);
    if let Some(comment) = comment {
        let comment_start = start + key.len();
        tokens.push(comment_start, comment_start + comment.len(), TokenKind::Comment);
    }
}

/// Whether the text before a mapping colon is a key: not empty, and not
/// a flow collection (`{a: 1}` is a value).
fn is_key(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty() && !key.starts_with(['[', '{'])
}

/// The bytes of a line's item dashes (`- `, `- - `) with the space after.
fn item_dashes_len(content: &str) -> usize {
    let mut rest = content;
    while let Some(after) = rest
        .strip_prefix('-')
        .filter(|after| after.starts_with([' ', '\t']))
    {
        rest = after.trim_start_matches([' ', '\t']);
    }
    content.len() - rest.len()
}

/// The length of a directive's name (`%YAML`) or a document marker
/// (`---`, `...`) starting a line.
fn document_mark(content: &str) -> Option<usize> {
    if content.starts_with('%') {
        return Some(content.find(char::is_whitespace).unwrap_or(content.len()));
    }
    ["---", "..."]
        .into_iter()
        .find(|mark| {
            content
                .strip_prefix(mark)
                .is_some_and(|after| after.chars().next().is_none_or(char::is_whitespace))
        })
        .map(str::len)
}

/// Tokens for `value`, which starts at byte `offset` of the line: its
/// anchor and tag first (an anchor named like a key, a tag like a
/// literal), then the node: an alias (named like a key), a block scalar's
/// indicator, a flow collection or a scalar, and any comment.
pub(super) fn highlight_value(tokens: &mut Tokens, offset: usize, value: &str) {
    let mut start = offset + value.len() - value.trim_start().len();
    let mut rest = value.trim_start();
    while let Some(length) = property_len(rest) {
        let kind = if rest.starts_with('&') {
            TokenKind::Key
        } else {
            TokenKind::Literal
        };
        tokens.push(start, start + length, kind);
        let after = &rest[length..];
        start += length + after.len() - after.trim_start().len();
        rest = after.trim_start();
    }
    if rest.is_empty() {
        return;
    }
    // A flow collection: its entries, as on its lines after.
    if rest.starts_with(['[', '{']) {
        super::constructs::flow_tokens(rest, start, tokens);
        return;
    }
    let (scalar, comment) = split_yaml_comment(rest);
    let scalar_end = start + scalar.len();
    let kind = if scalar.starts_with('*') || is_block_indicator(scalar.trim_end()) {
        if scalar.starts_with('*') {
            TokenKind::Key
        } else {
            TokenKind::Punctuation
        }
    } else {
        scalar_kind(scalar)
    };
    if !scalar.trim().is_empty() {
        tokens.push(start, scalar_end, kind);
    }
    if let Some(comment) = comment {
        tokens.push(scalar_end, scalar_end + comment.len(), TokenKind::Comment);
    }
}

/// The length of the anchor (`&name`) or tag (`!tag`, `!!str`,
/// `!<uri>`) starting `text`, followed by a space or the end.
fn property_len(text: &str) -> Option<usize> {
    if !text.starts_with(['&', '!']) {
        return None;
    }
    Some(text.find([' ', '\t']).unwrap_or(text.len()))
}

/// Whether `text` is a block scalar's header: `|` or `>`, then an
/// indentation digit and a chomping `+` or `-` in either order.
fn is_block_indicator(text: &str) -> bool {
    text.strip_prefix(['|', '>']).is_some_and(|rest| {
        rest.len() <= 2
            && rest.chars().all(|mark| matches!(mark, '1'..='9' | '+' | '-'))
            && rest.matches(['+', '-']).count() <= 1
    })
}

pub(super) fn scalar_kind(scalar: &str) -> TokenKind {
    let scalar = scalar.trim_end();
    if is_literal(scalar) {
        TokenKind::Literal
    } else if scalar.starts_with('\'') || scalar.starts_with('"') {
        TokenKind::String
    } else {
        TokenKind::Value
    }
}

/// Whether `scalar` is a boolean, null or number of YAML's core schema.
fn is_literal(scalar: &str) -> bool {
    let words = [
        "true", "True", "TRUE", "false", "False", "FALSE", "null", "Null", "NULL", "~",
    ];
    if words.contains(&scalar) || matches!(scalar, ".nan" | ".NaN" | ".NAN") {
        return true;
    }
    let unsigned = scalar.strip_prefix(['-', '+']).unwrap_or(scalar);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
        return true;
    }
    let digits = |text: &str, radix: u32| {
        !text.is_empty() && text.chars().all(|digit| digit.is_digit(radix))
    };
    if let Some(octal) = scalar.strip_prefix("0o") {
        return digits(octal, 8);
    }
    if let Some(hex) = scalar.strip_prefix("0x") {
        return digits(hex, 16);
    }
    decimal(unsigned)
}

/// `[0-9]+(\.[0-9]*)?` or `\.[0-9]+`, then an optional exponent.
fn decimal(text: &str) -> bool {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let all_digits = |part: &str| part.chars().all(|digit| digit.is_ascii_digit());
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mantissa_ok = all_digits(whole)
        && all_digits(fraction)
        && (!whole.is_empty() || !fraction.is_empty());
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        !exponent.is_empty() && all_digits(exponent)
    });
    mantissa_ok && exponent_ok
}
