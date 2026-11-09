//! A one-line container (`{path: /h, port: 1}`, `[ 'a', "b" ]`) written
//! again after an edit, in the host's style: untouched items keep their
//! text, changed ones keep their quoting, new ones are written like their
//! sibling, and the spacing inside the brackets stays.

use super::super::language::ConfigLanguage;
use super::comparable;
use serde_json::Value;

/// `value` written where `written` (one `{…}` or `[…]`) was, or `None` when
/// `written` holds no item to take the style from.
pub(super) fn restyle(
    language: &dyn ConfigLanguage,
    value: &Value,
    written: &str,
) -> Option<String> {
    let (open, close) = match (value, written.chars().next()?) {
        (Value::Object(_), '{') => ('{', '}'),
        (Value::Array(_), '[') => ('[', ']'),
        _ => return None,
    };
    let inner = written.strip_prefix(open)?.strip_suffix(close)?;
    let raw = split_items(inner)?;
    if raw.is_empty() {
        return None;
    }
    // Each item with the spacing written around it; the space just inside
    // the brackets stays there.
    let last = raw.len() - 1;
    let lead = &raw[0][..raw[0].len() - raw[0].trim_start().len()];
    let trail = &raw[last][raw[last].trim_end().len()..];
    let items: Vec<(&str, &str, &str)> = raw
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let text = item.trim();
            let start = item.len() - item.trim_start().len();
            let end = item.trim_end().len();
            let before = if index == 0 { "" } else { &item[..start] };
            let after = if index == last { "" } else { &item[end..] };
            (before, text, after)
        })
        .collect();
    // The spacing a new item takes: the item before it's, else none in a
    // compact container (`{"a":1}`), else one space.
    let compact = lead.is_empty() && !items[0].1.contains(": ") && !items[0].1.contains(" =");
    let gap = |position: usize| -> &str {
        match items.get(position.min(last)) {
            Some((before, _, _)) if position > 0 && !before.is_empty() => before,
            _ if items.len() > 1 => items[1].0,
            _ if compact => "",
            _ => " ",
        }
    };
    let mut out: Vec<String> = Vec::new();
    let place = |out: &mut Vec<String>, before: &str, text: String, after: &str| {
        let before = if out.is_empty() {
            ""
        } else if before.is_empty() {
            gap(out.len())
        } else {
            before
        };
        out.push(format!("{before}{text}{after}"));
    };
    match value {
        Value::Object(map) => {
            let entries: Vec<(String, &str, &str)> = items
                .iter()
                .map(|(_, item, _)| entry_parts(item))
                .collect::<Option<_>>()?;
            let (_, model_head, model_value) = entries.first()?;
            for (key, child) in map {
                match entries.iter().position(|(name, _, _)| name == key) {
                    Some(at) => {
                        let (_, head, old) = &entries[at];
                        let (before, _, after) = items[at];
                        place(
                            &mut out,
                            before,
                            format!("{head}{}", item_text(language, child, old)),
                            after,
                        );
                    }
                    None => {
                        let separator =
                            &model_head[model_head.trim_end_matches([' ', '\t', ':', '=']).len()..];
                        let key = super::key_like(language, key, model_head)?;
                        let text = fresh_text(language, child, model_value, written);
                        place(&mut out, "", format!("{key}{separator}{text}"), "");
                    }
                }
            }
        }
        Value::Array(values) => {
            let mut used = vec![false; items.len()];
            for (index, child) in values.iter().enumerate() {
                let reused = (0..items.len())
                    .find(|at| !used[*at] && same_written(language, items[*at].1, child));
                match reused {
                    Some(at) => {
                        used[at] = true;
                        let (before, text, after) = items[at];
                        place(&mut out, before, text.to_string(), after);
                    }
                    // In place of the item that was there, else like the last.
                    None => match items.get(index) {
                        Some((before, text, after)) if !used[index] => {
                            used[index] = true;
                            place(&mut out, before, item_text(language, child, text), after);
                        }
                        _ => place(
                            &mut out,
                            "",
                            fresh_text(language, child, items[last].1, written),
                            "",
                        ),
                    },
                }
            }
        }
        _ => return None,
    }
    if out.is_empty() {
        return Some(format!("{open}{close}"));
    }
    Some(format!("{open}{lead}{}{trail}{close}", out.join(",")))
}

/// `value` where the item `written` was: nested one-line containers are
/// restyled too, scalars keep their quoting.
fn item_text(language: &dyn ConfigLanguage, value: &Value, written: &str) -> String {
    if same_written(language, written, value) {
        return written.to_string();
    }
    restyle(language, value, written).unwrap_or_else(|| language.format_value_like(value, written))
}

/// Whether `written` already writes `value` (however a number is spelled:
/// `7.835e3` for 7835).
fn same_written(language: &dyn ConfigLanguage, written: &str, value: &Value) -> bool {
    let formatted = language.format_value(value);
    comparable(language, written) == comparable(language, &formatted)
        || (!value.is_object() && !value.is_array() && language.scalar_reads_as(written, value))
        || value.is_number()
            && super::canonical_number(written.trim())
                .is_some_and(|number| Some(number) == super::canonical_number(&formatted))
}

/// A new item, written like its sibling `model` when both are scalars, and
/// a container like the one it goes in (`container`: compact JSON stays so).
fn fresh_text(
    language: &dyn ConfigLanguage,
    value: &Value,
    model: &str,
    container: &str,
) -> String {
    if value.is_object() || value.is_array() || model.starts_with(['{', '[']) {
        language.format_value_like(value, container)
    } else {
        language.format_value_like(value, model)
    }
}

/// An entry `key: value` (or `key = value`): its key, the text up to its
/// value, and its value.
fn entry_parts(item: &str) -> Option<(String, &str, &str)> {
    let mut quote = None;
    for (index, character) in item.char_indices() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, ':' | '=') => {
                let key = item[..index].trim().trim_matches(['"', '\'']).to_string();
                let value_start =
                    index + 1 + (item[index + 1..].len() - item[index + 1..].trim_start().len());
                return Some((key, &item[..value_start], &item[value_start..]));
            }
            _ => {}
        }
    }
    None
}

/// The items of a container's inside, split at its top-level commas, each
/// with the spacing before it. `None` when a string or bracket is open.
fn split_items(inner: &str) -> Option<Vec<&str>> {
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    let (mut depth, mut quote, mut start, mut escaped) = (0usize, None, 0, false);
    for (index, character) in inner.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && open == '"' {
                escaped = true;
            } else if character == open {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                items.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    let last = &inner[start..];
    // A trailing comma (`[a, b, ]`) ends the list without another item.
    if !last.trim().is_empty() {
        items.push(last);
    }
    Some(items)
}

/// Whether `previous` writes a container closed by `close` with items on
/// one line (`tags: [a, b]`) other than at `skip` (a byte range): a sign
/// its empty ones (`[]`) are filled on one line too.
pub(super) fn writes_filled_on_one_line(
    language: &dyn ConfigLanguage,
    previous: &str,
    close: char,
    skip: std::ops::Range<usize>,
) -> bool {
    let Some(open) = language
        .config()
        .brackets
        .iter()
        .find_map(|(open, closer)| (*closer == close).then_some(*open))
    else {
        return false;
    };
    let mut offset = 0;
    for line in previous.split_inclusive('\n') {
        let code = &line[..super::comment_start(language, line)];
        let filled = code.char_indices().any(|(at, character)| {
            let start = offset + at;
            character == open
                && !skip.contains(&start)
                && super::super::inline::matching_close(&code[at..], close)
                    .is_some_and(|end| !code[at + 1..at + end].trim().is_empty())
        });
        if filled {
            return true;
        }
        offset += line.len();
    }
    false
}

/// Whether `after` (a map or list `written` one entry per line) adds or
/// changes an entry holding a container with entries of its own that is
/// written over lines as its siblings are, not on one line inside them: in
/// a language whose collections over lines are its block form (JSON), where
/// no sibling container is written on one line (HCL's `{ name = "a" },`),
/// and in a list only beside one written over lines.
pub(super) fn adds_containers_over_lines(
    language: &dyn ConfigLanguage,
    written: &str,
    before: &Value,
    after: &Value,
) -> bool {
    if language.indent_is_structure() {
        return false;
    }
    let closer_of = |open: char| {
        language
            .config()
            .brackets
            .iter()
            .find_map(|(opener, close)| (*opener == open).then_some(*close))
    };
    let lines: Vec<&str> = written.lines().collect();
    let inner = lines
        .get(1..lines.len().saturating_sub(1))
        .unwrap_or_default();
    let code = |line: &&str| {
        line[..super::comment_start(language, line)]
            .trim()
            .to_string()
    };
    let one_line = inner.iter().map(code).any(|code| {
        code.char_indices().any(|(at, character)| {
            let Some(close) = closer_of(character) else {
                return false;
            };
            super::super::inline::matching_close(&code[at..], close)
                .is_some_and(|end| !code[at + 1..at + end].trim().is_empty())
        })
    });
    let opened = inner.iter().map(code).any(|code| {
        code.chars()
            .next_back()
            .is_some_and(|last| closer_of(last).is_some())
    });
    if one_line || (after.is_array() && !opened) {
        return false;
    }
    let filled = |value: &Value| match value {
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        _ => false,
    };
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => after
            .iter()
            .any(|(key, value)| filled(value) && before.get(key) != Some(value)),
        (Value::Array(before), Value::Array(after)) => after
            .iter()
            .any(|value| filled(value) && !before.contains(value)),
        _ => false,
    }
}
