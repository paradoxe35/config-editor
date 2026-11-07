//! Writing a value in place of one the host wrote, in the host's style:
//! plain stays plain, quotes keep their kind, block scalars stay blocks.

use super::*;

/// `value` written where `written` was. Anything the style cannot hold is
/// written as [`yaml_value`] does.
pub(super) fn yaml_value_like(language: &YamlBehavior, value: &Value, written: &str) -> String {
    // A value made empty again is written as YAML writes an empty value
    // (`key:`), unless the host spells its nulls out.
    if value.is_null() && !matches!(written.trim(), "null" | "Null" | "NULL" | "~") {
        return String::new();
    }
    let Value::String(text) = value else {
        return yaml_value(value);
    };
    let written = written.trim_start();
    let reads_back = |candidate: &str| {
        // Both as a block value and inside a flow collection.
        language.parse(&format!("k: {candidate}\n")).ok() == Some(serde_json::json!({ "k": text }))
            && language.parse(&format!("k: [{candidate}]\n")).ok() == Some(serde_json::json!({ "k": [text] }))
    };
    match written.chars().next() {
        Some('\'') if !text.contains('\n') => format!("'{}'", text.replace('\'', "''")),
        Some('"') => serde_json::to_string(text).unwrap_or_else(|_| yaml_value(value)),
        Some('|' | '>') => block_scalar(written, text).unwrap_or_else(|| yaml_value(value)),
        Some(_) if !text.is_empty() && reads_back(text) => text.clone(),
        _ => yaml_value(value),
    }
}

/// `text` as the block scalar `written` (`>-` then its indented lines), with
/// the chomping its line breaks need; `None` when it cannot be one.
fn block_scalar(written: &str, text: &str) -> Option<String> {
    let mut lines = written.split('\n');
    let header = lines.next()?;
    let indent: String = lines
        .find(|line| !line.trim().is_empty())?
        .chars()
        .take_while(|character| *character == ' ')
        .collect();
    let body = text.strip_suffix('\n').unwrap_or(text);
    if indent.is_empty()
        || body.contains("\n\n")
        || body.ends_with('\n')
        || body.split('\n').any(|line| line.starts_with([' ', '\t']) || line.trim_end() != line)
        || body.is_empty()
    {
        return None;
    }
    let indicator_end = header
        .find(|character: char| !matches!(character, '|' | '>' | '-' | '+' | '0'..='9'))
        .unwrap_or(header.len());
    let mut style = header[..1].to_string();
    // Folded text keeps its line breaks only as literal text.
    if style == ">" && body.contains('\n') {
        style = "|".to_string();
    }
    let chomping = if text.ends_with('\n') { "" } else { "-" };
    let mut out = format!("{style}{chomping}{}", &header[indicator_end..]);
    for line in body.split('\n') {
        out.push('\n');
        out.push_str(&indent);
        out.push_str(line);
    }
    Some(out)
}
