//! A regular expression's replacement, read as VS Code reads it (the rules
//! of JavaScript's `String.replace`, plus `$0` and `${…}`):
//!
//! - `$1`–`$99`: a group. Two digits when that group exists, else one digit
//!   and the rest as text (`$1_host` is group 1, then `_host`; `$10` with
//!   one group is group 1, then `0`); a group the pattern lacks stays as
//!   written.
//! - `$&` and `$0`: the whole match; `$$`: a `$`.
//! - `$<name>` and `${name}` (or `${1}`): a group by name or number.
//! - `\n`, `\t`, `\\`: a newline, a tab, a backslash.
//!
//! Anything else is copied as written. A group that took no part in the
//! match is empty.

use crate::editor::pattern_engine::{Captures, Regex};

/// `template` with its references to `captures` (a match of `regex`)
/// filled in.
pub(super) fn expand(template: &str, regex: &Regex, captures: &Captures<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(character) = rest.chars().next() {
        let consumed = match character {
            '$' => dollar(&rest[1..], regex, captures, &mut out),
            '\\' => backslash(&rest[1..], &mut out),
            _ => None,
        };
        let taken = consumed.map_or_else(
            || {
                out.push(character);
                character.len_utf8()
            },
            |after| 1 + after,
        );
        rest = &rest[taken..];
    }
    out
}

/// After a `$`: the reference it starts, written to `out`, and the bytes it
/// takes after the `$`; `None` when it starts none.
fn dollar(after: &str, regex: &Regex, captures: &Captures<'_>, out: &mut String) -> Option<usize> {
    let bytes = after.as_bytes();
    match *bytes.first()? {
        b'$' => {
            out.push('$');
            Some(1)
        }
        b'&' | b'0' => {
            push_group(captures, 0, out);
            Some(1)
        }
        b'1'..=b'9' => {
            let one = usize::from(bytes[0] - b'0');
            let two = bytes
                .get(1)
                .filter(|digit| digit.is_ascii_digit())
                .map(|digit| one * 10 + usize::from(digit - b'0'));
            if let Some(two) = two.filter(|&group| group < captures.len()) {
                push_group(captures, two, out);
                Some(2)
            } else if one < captures.len() {
                push_group(captures, one, out);
                Some(1)
            } else {
                None
            }
        }
        open @ (b'<' | b'{') => {
            let close = if open == b'<' { '>' } else { '}' };
            let length = after[1..].find(close)?;
            let reference = &after[1..1 + length];
            // A group the pattern has but the match skipped is empty; one
            // it lacks stays as written.
            let group = match reference.parse::<usize>() {
                Ok(index) if index < captures.len() => captures.get(index),
                Ok(_) => return None,
                Err(_)
                    if regex
                        .capture_names()
                        .flatten()
                        .any(|name| name == reference) =>
                {
                    captures.name(reference)
                }
                Err(_) => return None,
            };
            out.push_str(group.map_or("", |found| found.as_str()));
            Some(length + 2)
        }
        _ => None,
    }
}

fn push_group(captures: &Captures<'_>, index: usize, out: &mut String) {
    if let Some(found) = captures.get(index) {
        out.push_str(found.as_str());
    }
}

/// After a `\`: the escape it starts and the bytes it takes, or `None`.
fn backslash(after: &str, out: &mut String) -> Option<usize> {
    let escaped = match after.as_bytes().first()? {
        b'n' => '\n',
        b't' => '\t',
        b'\\' => '\\',
        _ => return None,
    };
    out.push(escaped);
    Some(1)
}
