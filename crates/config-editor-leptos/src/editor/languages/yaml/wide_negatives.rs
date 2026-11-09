//! A plain negative integer past 64 bits (`-99999999999999999999`): the
//! parser reads it as a string, where the positive one is an error. Both
//! are the same error.

use super::*;

/// The error for the first string of `value` written plain in `source` as
/// a negative integer past 64 bits.
pub(super) fn first_wide_negative(source: &str, value: &Value) -> Option<ParseError> {
    let mut strings = Vec::new();
    wide_negative_strings(value, &mut strings);
    strings.into_iter().find_map(|text| {
        let at = plain_at(source, text)?;
        let line = source[..at].matches('\n').count() + 1;
        let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
        Some(ParseError {
            message: super::super::wide_integers::message(text),
            line: Some(line),
            column: Some(source[line_start..at].chars().count() + 1),
        })
    })
}

fn wide_negative_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => {
            let digits = text.strip_prefix('-').unwrap_or_default();
            if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) && text.parse::<i64>().is_err() {
                out.push(text);
            }
        }
        Value::Object(map) => map.values().for_each(|child| wide_negative_strings(child, out)),
        Value::Array(items) => items.iter().for_each(|item| wide_negative_strings(item, out)),
        _ => {}
    }
}

/// Where `literal` is written plain in `source`: not quoted, a whole
/// scalar, outside comments.
fn plain_at(source: &str, literal: &str) -> Option<usize> {
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let code = code_before_comment(line.trim_end_matches('\n'));
        let mut from = 0;
        while let Some(found) = code[from..].find(literal) {
            let start = from + found;
            let end = start + literal.len();
            let before = code[..start].chars().next_back();
            let after = code[end..].chars().next();
            let bounded = before.is_none_or(|character| character.is_whitespace() || "[{,:".contains(character))
                && after.is_none_or(|character| character.is_whitespace() || "]},".contains(character));
            if bounded {
                return Some(offset + start);
            }
            from = end;
        }
        offset += line.len();
    }
    None
}
