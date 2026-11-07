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
        let before = if out.is_empty() { "" } else if before.is_empty() { gap(out.len()) } else { before };
        out.push(format!("{before}{text}{after}"));
    };
    match value {
        Value::Object(map) => {
            let entries: Vec<(String, &str, &str)> =
                items.iter().map(|(_, item, _)| entry_parts(item)).collect::<Option<_>>()?;
            let (_, model_head, model_value) = entries.first()?;
            for (key, child) in map {
                match entries.iter().position(|(name, _, _)| name == key) {
                    Some(at) => {
                        let (_, head, old) = &entries[at];
                        let (before, _, after) = items[at];
                        place(&mut out, before, format!("{head}{}", item_text(language, child, old)), after);
                    }
                    None => {
                        let separator = &model_head[model_head.trim_end_matches([' ', '\t', ':', '=']).len()..];
                        let quoted = model_head.starts_with(['"', '\'']) || !is_bare(key);
                        let key = if quoted { serde_json::to_string(key).ok()? } else { key.clone() };
                        let text = fresh_text(language, child, model_value, written);
                        place(&mut out, "", format!("{key}{separator}{text}"), "");
                    }
                }
            }
        }
        Value::Array(values) => {
            let mut used = vec![false; items.len()];
            for (index, child) in values.iter().enumerate() {
                let wanted = comparable(language, &language.format_value(child));
                let reused = (0..items.len()).find(|at| !used[*at] && comparable(language, items[*at].1) == wanted);
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
                        _ => place(&mut out, "", fresh_text(language, child, items[last].1, written), ""),
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
    if comparable(language, written) == comparable(language, &language.format_value(value)) {
        return written.to_string();
    }
    restyle(language, value, written).unwrap_or_else(|| language.format_value_like(value, written))
}

/// A new item, written like its sibling `model` when both are scalars, and
/// a container like the one it goes in (`container`: compact JSON stays so).
fn fresh_text(language: &dyn ConfigLanguage, value: &Value, model: &str, container: &str) -> String {
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
