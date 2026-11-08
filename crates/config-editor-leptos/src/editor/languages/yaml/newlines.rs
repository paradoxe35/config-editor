//! Strings of line breaks only (`"\n"`): no block scalar holds them (a
//! `|+` block of blank lines reads back as `""`), so they are written
//! double-quoted with escapes, `"\n"`, which reads back the same.

use serde_json::Value;

/// `value` with each string of line breaks only replaced by a marker
/// `write` can find, and the markers with what they stand for; `None`
/// when it holds no such string.
pub(super) fn marked(value: &Value) -> Option<(Value, Vec<(String, String)>)> {
    let mut found = false;
    probe(value, &mut found);
    if !found {
        return None;
    }
    // A marker no text of the value holds.
    let text = value.to_string();
    let mut stem = String::from("yamlnewlines");
    while text.contains(&stem) {
        stem.push('x');
    }
    let mut markers = Vec::new();
    let marked = replace(value, &stem, &mut markers);
    Some((marked, markers))
}

/// `text` (written with the markers of [`marked`]) with each marker
/// written as its double-quoted string.
pub(super) fn written(mut text: String, markers: &[(String, String)]) -> String {
    for (marker, string) in markers.iter().rev() {
        let quoted = serde_json::to_string(string).unwrap_or_default();
        text = text.replace(marker, &quoted);
    }
    text
}

fn only_breaks(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| matches!(character, '\n' | '\r'))
}

fn probe(value: &Value, found: &mut bool) {
    match value {
        Value::String(text) if only_breaks(text) => *found = true,
        Value::Object(map) => map.values().for_each(|child| probe(child, found)),
        Value::Array(items) => items.iter().for_each(|item| probe(item, found)),
        _ => {}
    }
}

fn replace(value: &Value, stem: &str, markers: &mut Vec<(String, String)>) -> Value {
    match value {
        Value::String(text) if only_breaks(text) => {
            let marker = format!("{stem}{}", markers.len());
            markers.push((marker.clone(), text.clone()));
            Value::String(marker)
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), replace(child, stem, markers)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| replace(item, stem, markers))
                .collect(),
        ),
        other => other.clone(),
    }
}
