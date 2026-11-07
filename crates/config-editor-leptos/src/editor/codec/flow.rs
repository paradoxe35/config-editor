//! A one-line container (`{path: /h, port: 1}`, `[ 'a', "b" ]`) written
//! again after an edit, in the host's style: untouched items keep their
//! text, changed ones keep their quoting, new ones are written like their
//! sibling, and the spacing inside the brackets stays.

use super::super::language::ConfigLanguage;
use super::comparable;
use serde_json::Value;

/// `value` written where `written` (one `{…}` or `[…]`) was, or `None` when
/// `written` holds no item to take the style from.
pub(super) fn restyle(language: &dyn ConfigLanguage, value: &Value, written: &str) -> Option<String> {
    let (open, close) = match (value, written.chars().next()?) {
        (Value::Object(_), '{') => ('{', '}'),
        (Value::Array(_), '[') => ('[', ']'),
        _ => return None,
    };
    let inner = written.strip_prefix(open)?.strip_suffix(close)?;
    let items = split_items(inner)?;
    let first = items.first()?;
    // The spacing just inside the brackets, and after each comma.
    let lead = &inner[..inner.len() - inner.trim_start().len()];
    let trail = &inner[inner.trim_end().len()..];
    let gap = items
        .get(1)
        .map_or(" ", |item| &item[..item.len() - item.trim_start().len()]);
    let items: Vec<&str> = items.iter().map(|item| item.trim()).collect();
    let mut out: Vec<String> = Vec::new();
    match value {
        Value::Object(map) => {
            let entries: Vec<(String, &str, &str)> = items
                .iter()
                .map(|item| entry_parts(item))
                .collect::<Option<_>>()?;
            let model = entries.first()?;
            for (key, child) in map {
                let old = entries.iter().find(|(name, _, _)| name == key);
                let text = match old {
                    Some((_, head, written)) => format!("{head}{}", item_text(language, child, written)),
                    None => {
                        let (_, head, written) = model;
                        let separator = &head[head.trim_end_matches([' ', '\t', ':', '=']).len()..];
                        let quoted = head.starts_with(['"', '\'']) || !is_bare(key);
                        let key = if quoted { serde_json::to_string(key).ok()? } else { key.clone() };
                        format!("{key}{separator}{}", fresh_text(language, child, written))
                    }
                };
                out.push(text);
            }
        }
        Value::Array(values) => {
            let mut used = vec![false; items.len()];
            for (index, child) in values.iter().enumerate() {
                let wanted = comparable(language, &language.format_value(child));
                let reused = (0..items.len()).find(|at| !used[*at] && comparable(language, items[*at]) == wanted);
                let text = match reused {
                    Some(at) => {
                        used[at] = true;
                        items[at].to_string()
                    }
                    // In place of the item that was there, else like the last.
                    None => match items.get(index) {
                        Some(written) if !used[index] => item_text(language, child, written),
                        _ => fresh_text(language, child, items.last().unwrap_or(first)),
                    },
                };
                out.push(text);
            }
        }
        _ => return None,
    }
    if out.is_empty() {
        return Some(format!("{open}{close}"));
    }
    Some(format!("{open}{lead}{}{trail}{close}", out.join(&format!(",{gap}"))))
}

/// `value` where the item `written` was: nested one-line containers are
/// restyled too, scalars keep their quoting.
fn item_text(language: &dyn ConfigLanguage, value: &Value, written: &str) -> String {
    if comparable(language, written) == comparable(language, &language.format_value(value)) {
        return written.to_string();
    }
    restyle(language, value, written).unwrap_or_else(|| language.format_value_like(value, written))
}

/// A new item, written like its sibling `model` when both are scalars.
fn fresh_text(language: &dyn ConfigLanguage, value: &Value, model: &str) -> String {
    if value.is_object() || value.is_array() || model.starts_with(['{', '[']) {
        language.format_value(value)
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
                let value_start = index + 1 + (item[index + 1..].len() - item[index + 1..].trim_start().len());
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

fn is_bare(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}
