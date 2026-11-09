//! Lines and values compared however they are written: the structural
//! patch matches the serializer's lines to the host's by these.

use super::super::flow_syntax::Flow;
use super::super::language::ConfigLanguage;

/// A line as the structural patch compares it: trimmed, without quotes and
/// without its trailing comment (as the language finds it).
pub(super) fn comparable(language: &dyn ConfigLanguage, line: &str) -> String {
    let code = line[..comment_start(language, line)].trim();
    // Written forms the language reads the same (its own rules: `\u00e9`
    // for `é`).
    let code = language.comparable_line(code);
    let code = code.as_ref();
    // A number however it is written (`0x10`, `1.5e1`, `1_000`) is the same
    // line as the serializer's `16`, `15.0`, `1000`.
    let flow = Flow::of(language);
    let item_separator = flow.separator();
    let spaced: Vec<String> = language
        .config()
        .separators
        .iter()
        .map(|separator| format!("{separator} "))
        .collect();
    let numbered;
    let code = match spaced.iter().find_map(|separator| {
        code.split_once(separator.as_str())
            .map(|(key, value)| (key, separator, value))
    }) {
        Some((key, separator, value)) => {
            let trailing = item_separator.filter(|item| value.trim_end().ends_with(*item));
            let bare = item_separator.map_or(value, |item| value.trim_end_matches(item));
            match canonical_number(bare.trim()) {
                Some(number) => {
                    numbered = format!(
                        "{key}{separator}{number}{}",
                        trailing.map(String::from).unwrap_or_default()
                    );
                    numbered.as_str()
                }
                None => code,
            }
        }
        None => code,
    };
    let unquoted: String = code
        .chars()
        .filter(|character| flow.quote(*character).is_none())
        .collect();
    // A line of quotes alone (a multi-line string's `"""`) still is a line.
    if unquoted.is_empty() {
        code.to_string()
    } else {
        unquoted
    }
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

/// The trailing comment of `line` with the spacing written before it
/// (`    # pc`), for a line that takes it over; `""` without a comment.
pub(super) fn spaced_trailing_comment<'a>(language: &dyn ConfigLanguage, line: &'a str) -> &'a str {
    let start = comment_start(language, line);
    if line[start..].trim_end().is_empty() {
        return "";
    }
    let code_end = line[..start].trim_end().len();
    let spaced = line[code_end..].trim_end();
    if spaced.starts_with([' ', '\t']) {
        spaced
    } else {
        &line[start..]
    }
}

/// Where the trailing comment of `line` starts, or its length.
pub(super) fn comment_start(language: &dyn ConfigLanguage, line: &str) -> usize {
    language
        .comment_start(line)
        .filter(|start| *start <= line.len() && line.is_char_boundary(*start))
        .unwrap_or(line.len())
}
