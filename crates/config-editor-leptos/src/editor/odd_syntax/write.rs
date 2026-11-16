//! The odd language's writer: blocks for maps and lists, `( … | … )` and
//! `< … := … | … >` for empty ones and for values written on one line.

use serde_json::Value;

use super::read::scalar;

/// `value` (a map) as a document.
pub(super) fn document(value: &Value) -> Result<String, String> {
    let Value::Object(map) = value else {
        return Err("an odd document is a map".to_string());
    };
    let mut out = String::new();
    for (key, child) in map {
        entry(&mut out, 0, &format!("{} :=", key_text(key)), child);
    }
    Ok(out)
}

/// One entry or item: `head` then its value, inline or as a block below.
fn entry(out: &mut String, indent: usize, head: &str, value: &Value) {
    let pad = "  ".repeat(indent);
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push_str(&format!("{pad}{head}\n"));
            for (key, child) in map {
                entry(out, indent + 1, &format!("{} :=", key_text(key)), child);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str(&format!("{pad}{head}\n"));
            for item in items {
                entry(out, indent + 1, "@", item);
            }
        }
        _ => out.push_str(&format!("{pad}{head} {}\n", inline(value))),
    }
}

/// `value` written on one line.
pub(super) fn inline(value: &Value) -> String {
    match value {
        Value::Null => "nil".to_string(),
        Value::Bool(true) => "yes".to_string(),
        Value::Bool(false) => "no".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => string(text),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(inline).collect();
            if items.is_empty() {
                "()".to_string()
            } else {
                format!("({})", items.join(" | "))
            }
        }
        Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(key, child)| format!("{} := {}", key_text(key), inline(child)))
                .collect();
            if entries.is_empty() {
                "<>".to_string()
            } else {
                format!("<{}>", entries.join(" | "))
            }
        }
    }
}

/// A string bare where it reads back as itself, else quoted.
fn string(text: &str) -> String {
    let bare = !text.is_empty()
        && text.starts_with(|c: char| c.is_alphabetic() || c == '_')
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\''))
        && scalar(text) == Value::String(text.to_string());
    if bare {
        return text.to_string();
    }
    let mut out = String::from('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn key_text(key: &str) -> String {
    let bare = key.starts_with(|c: char| c.is_alphabetic() || c == '_')
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'));
    if bare {
        key.to_string()
    } else {
        string_quoted(key)
    }
}

fn string_quoted(text: &str) -> String {
    let written = string(text);
    if written.starts_with('"') {
        written
    } else {
        format!("\"{written}\"")
    }
}
