//! Dot's reader and writer, as a host would write them in JavaScript.

use serde_json::{Map, Value, json};

use super::super::language::ParseError;

fn error(message: &str, line: usize) -> ParseError {
    ParseError {
        message: message.to_string(),
        line: Some(line),
        column: Some(1),
    }
}

/// `line` without its `#` comment (outside quotes).
fn without_comment(line: &str) -> &str {
    let mut quoted = false;
    for (at, character) in line.char_indices() {
        match character {
            '\'' => quoted = !quoted,
            '#' if !quoted => return &line[..at],
            _ => {}
        }
    }
    line
}

/// `text` split at `separator` outside quotes and brackets.
fn split_top(text: &str, separator: char) -> Vec<String> {
    let (mut out, mut current, mut depth, mut quoted) = (Vec::new(), String::new(), 0i32, false);
    for character in text.chars() {
        if quoted {
            current.push(character);
            quoted = character != '\'';
            continue;
        }
        match character {
            '\'' => quoted = true,
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            _ => {}
        }
        if character == separator && depth == 0 {
            out.push(std::mem::take(&mut current));
            continue;
        }
        current.push(character);
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn scalar(text: &str) -> Result<Value, String> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('\'') {
        let end = rest.find('\'').ok_or("unterminated quote")?;
        return Ok(json!(&rest[..end]));
    }
    Ok(match text {
        "true" => json!(true),
        "false" => json!(false),
        _ => match text.parse::<i64>() {
            Ok(number) => json!(number),
            Err(_) => match text.parse::<f64>() {
                Ok(number)
                    if text
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '.' || c == '-') =>
                {
                    json!(number)
                }
                _ => json!(text),
            },
        },
    })
}

fn value(text: &str) -> Result<Value, String> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix('<') {
        let inner = inner.strip_suffix('>').ok_or("unclosed <")?;
        let mut map = Map::new();
        for part in split_top(inner, ';') {
            let (key, rest) = part.split_once(':').ok_or("expected key: value")?;
            map.insert(key.trim().to_string(), value(rest)?);
        }
        return Ok(Value::Object(map));
    }
    if let Some(inner) = text.strip_prefix('(') {
        let inner = inner.strip_suffix(')').ok_or("unclosed (")?;
        return split_top(inner, ';')
            .iter()
            .map(|item| value(item))
            .collect();
    }
    scalar(text)
}

pub(super) fn document(source: &str) -> Result<Value, ParseError> {
    let mut out = Map::new();
    let mut list: Option<String> = None;
    for (index, raw) in source.split('\n').enumerate() {
        let line = without_comment(raw);
        if line.trim().is_empty() {
            continue;
        }
        let trimmed = line.trim_start();
        let item = trimmed.len() < line.len()
            && trimmed
                .strip_prefix('*')
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '));
        if item {
            let key = list
                .as_ref()
                .ok_or_else(|| error("item outside a list", index + 1))?;
            let item = value(&trimmed[1..]).map_err(|message| error(&message, index + 1))?;
            if let Some(Value::Array(items)) = out.get_mut(key) {
                items.push(item);
            }
            continue;
        }
        let (key, rest) = line
            .split_once(':')
            .ok_or_else(|| error("expected key: value", index + 1))?;
        let key = key.trim().to_string();
        if rest.trim().is_empty() {
            out.insert(key.clone(), json!([]));
            list = Some(key);
            continue;
        }
        list = None;
        out.insert(
            key,
            value(rest).map_err(|message| error(&message, index + 1))?,
        );
    }
    Ok(Value::Object(out))
}

fn write(value: &Value) -> Result<String, String> {
    Ok(match value {
        Value::Array(items) => format!(
            "({})",
            items
                .iter()
                .map(write)
                .collect::<Result<Vec<_>, _>>()?
                .join("; ")
        ),
        Value::Object(map) => format!(
            "<{}>",
            map.iter()
                .map(|(key, child)| Ok(format!("{key}: {}", write(child)?)))
                .collect::<Result<Vec<_>, String>>()?
                .join("; ")
        ),
        Value::String(text) if text.contains('\'') => {
            return Err("a quote cannot be written".to_string());
        }
        Value::String(text) => format!("'{text}'"),
        other => other.to_string(),
    })
}

pub(super) fn write_document(value: &Value) -> Result<String, String> {
    let Value::Object(map) = value else {
        return Err("the document is a map".to_string());
    };
    let mut out = String::new();
    for (key, child) in map {
        match child {
            Value::Array(items) if items.iter().any(|item| item.is_object()) => {
                out.push_str(&format!("{key}:\n"));
                for item in items {
                    out.push_str(&format!("  * {}\n", write(item)?));
                }
            }
            other => out.push_str(&format!("{key}: {}\n", write(other)?)),
        }
    }
    Ok(out)
}
