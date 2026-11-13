//! Integers past 64 bits, which a parser without big integers reads as a
//! float (silently not the number written): a syntax error, as YAML and
//! TOML make it.
#![cfg_attr(
    not(all(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )),
    allow(dead_code, reason = "not every language reads through each helper")
)]

use super::super::language::ParseError;
use serde_json::Value;

/// The pointer of the first number of `value` read as a float past the
/// 64-bit integers that `written` (the text at a pointer) shows as an
/// integer literal, with that text.
pub(super) fn first_past_64_bits(
    value: &Value,
    written: &dyn Fn(&str) -> Option<String>,
) -> Option<(String, String)> {
    let mut wide = Vec::new();
    floats_past_64_bits(value, String::new(), &mut wide);
    wide.into_iter().find_map(|pointer| {
        let text = written(&pointer)?;
        let digits = text.trim().strip_prefix('-').unwrap_or(text.trim());
        (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| (pointer, text.trim().to_string()))
    })
}

/// The message of the syntax error, with the bound `written` is past: the
/// 64-bit integers run from `i64::MIN` to `u64::MAX`, and to `i64::MAX`
/// where the parser takes no unsigned ones (TOML's).
pub(super) fn message(written: &str) -> String {
    let plain = written.replace('_', "");
    let bound = if plain.starts_with('-') {
        format!("below {}, the smallest 64-bit integer", i64::MIN)
    } else if plain.trim_start_matches('+').parse::<u64>().is_ok() {
        format!("above {}, the largest signed 64-bit integer", i64::MAX)
    } else {
        format!("above {}, the largest 64-bit integer", u64::MAX)
    };
    format!("the integer {written} is {bound}")
}

fn floats_past_64_bits(value: &Value, pointer: String, found: &mut Vec<String>) {
    match value {
        Value::Number(number) if number.is_f64() => {
            if number
                .as_f64()
                .is_some_and(|float| float.fract() == 0.0 && float.abs() >= 9.2e18)
            {
                found.push(pointer);
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                floats_past_64_bits(child, format!("{pointer}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                floats_past_64_bits(item, format!("{pointer}/{index}"), found);
            }
        }
        _ => {}
    }
}

/// A parser's error about a number it cannot hold (`u64 value was too
/// large`, `number out of range`, `floating-point number overflowed`, or a
/// syntax error inside `1e400`), said plainly: the number literal at
/// `line` and `column` (one-based, in characters; the error may point just
/// past it) past the 64-bit integers, or past the largest float. `None`
/// when no such literal is there.
pub(super) fn out_of_range(source: &str, line: usize, column: usize) -> Option<String> {
    literal_out_of_range(source, line, column).map(|(_, message)| message)
}

/// Where the number literal `error` (a parse of `source`) is about is
/// written, when the error is only that the number is past what the
/// editor holds exactly (see [`out_of_range`], [`message`]).
pub(super) fn unheld(source: &str, error: &ParseError) -> Option<std::ops::Range<usize>> {
    let (range, message) = literal_out_of_range(source, error.line?, error.column?)?;
    (message == error.message).then_some(range)
}

/// [`out_of_range`], and the literal's byte range.
fn literal_out_of_range(
    source: &str,
    line: usize,
    column: usize,
) -> Option<(std::ops::Range<usize>, String)> {
    let line_start = source
        .split_inclusive('\n')
        .take(line.checked_sub(1)?)
        .map(str::len)
        .sum::<usize>();
    let text: Vec<(usize, char)> = source.split('\n').nth(line - 1)?.char_indices().collect();
    let part = |character: char| {
        character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.' | '_')
    };
    let at = column.checked_sub(1)?.min(text.len());
    // The literal holding the column, or ending just before it.
    let probe = if text.get(at).is_some_and(|(_, character)| part(*character)) {
        at
    } else {
        at.checked_sub(1)?
    };
    if !text
        .get(probe)
        .is_some_and(|(_, character)| part(*character))
    {
        return None;
    }
    let start = (0..=probe)
        .rev()
        .take_while(|index| part(text[*index].1))
        .last()?;
    let end = (probe..text.len())
        .take_while(|index| part(text[*index].1))
        .last()?
        + 1;
    let literal: String = text[start..end]
        .iter()
        .map(|(_, character)| character)
        .collect();
    let bytes = line_start + text[start].0..line_start + text[end - 1].0 + 1;
    let plain = literal.replace('_', "");
    let digits = plain.strip_prefix(['-', '+']).unwrap_or(&plain);
    if !digits.starts_with(|character: char| character.is_ascii_digit()) {
        return None;
    }
    if digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return plain
            .parse::<i64>()
            .is_err()
            .then(|| (bytes, message(&literal)));
    }
    plain.parse::<f64>().is_ok_and(f64::is_infinite).then(|| {
        (
            bytes,
            format!("the number {literal} is past the largest float"),
        )
    })
}
