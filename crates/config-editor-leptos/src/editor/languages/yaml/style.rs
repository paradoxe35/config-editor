//! Writing a value in place of one the host wrote, in the host's style:
//! plain stays plain, quotes keep their kind, block scalars stay blocks.

use super::*;

/// `value` written where `written` was. Anything the style cannot hold is
/// written as [`yaml_value`] does.
use super::super::super::codec::non_finite;

pub(super) fn yaml_value_like(language: &YamlBehavior, value: &Value, written: &str) -> String {
    // A value made empty again is written as YAML writes an empty value
    // (`key:`), unless the host spells its nulls out.
    if value.is_null() && !matches!(written.trim(), "null" | "Null" | "NULL" | "~") {
        return String::new();
    }
    let Value::String(text) = value else {
        return yaml_value(value);
    };
    if non_finite::name(value).is_some() {
        return yaml_value(value);
    }
    // The text as written (a string holding a non-finite number's form
    // carries one more mark in the model).
    let text = non_finite::written(text);
    let written = written.trim_start();
    let reads_back = |candidate: &str| {
        // Both as a block value and inside a flow collection.
        language.parse(&format!("k: {candidate}\n")).ok() == Some(serde_json::json!({ "k": value }))
            && language.parse(&format!("k: [{candidate}]\n")).ok() == Some(serde_json::json!({ "k": [value] }))
    };
    match written.chars().next() {
        Some('\'') if !text.contains('\n') => format!("'{}'", text.replace('\'', "''")),
        Some('"') => super::super::escapes::like_written(
            serde_json::to_string(text).unwrap_or_else(|_| yaml_value(value)),
            written,
            super::super::escapes::Astral::Long,
        ),
        Some('|' | '>') => super::block_scalar::block_scalar(language, written, text).unwrap_or_else(|| yaml_value(value)),
        Some(_) if !text.is_empty() && reads_back(text) => text.to_string(),
        _ => yaml_value(value),
    }
}

/// `code` with written forms YAML reads the same made equal: a list item's
/// dash however far its text is (`-   a`), an anchor or tag on a value
/// (`pool: &p`, `n: !!int "7"`), an empty value and its null (`key: ~`).
pub(super) fn comparable_yaml_line(code: &str) -> std::borrow::Cow<'_, str> {
    let mut line = code.to_string();
    if let Some(rest) = line.strip_prefix('-').filter(|rest| rest.starts_with([' ', '\t'])) {
        line = format!("- {}", rest.trim_start());
    }
    if let Some((at, length)) = [": &", "- &", ": !", "- !"].iter().find_map(|marker| line.find(marker).map(|at| (at, marker.len()))) {
        let name_end = line[at + length..]
            .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-' | '!' | ':' | '.' | '/')))
            .map_or(line.len(), |offset| at + length + offset);
        if name_end > at + length {
            line = format!("{}{}", &line[..at + length - 1], line[name_end..].trim_start()).trim_end().to_string();
        }
    }
    for null in [": null", ": ~"] {
        if let Some(key) = line.strip_suffix(null) {
            line = format!("{key}:");
            break;
        }
    }
    if line == code { std::borrow::Cow::Borrowed(code) } else { std::borrow::Cow::Owned(line) }
}

/// The length of the anchors and tags (`&p `, `!!map `) before a value.
pub(super) fn node_properties_len(text: &str) -> usize {
    let mut length = 0;
    while text[length..].starts_with(['&', '!']) {
        let token = text[length..].find([' ', '\t']).unwrap_or(text.len() - length);
        let rest = &text[length + token..];
        let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if spaces == 0 {
            return 0;
        }
        length += token + spaces;
    }
    length
}
