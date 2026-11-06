//! Writing a JSON value as this language.

use super::*;

/// Document-level framing an XML text carries outside its data.
#[derive(Default)]
pub(super) struct XmlFraming {
    pub(super) declaration: Option<String>,
    pub(super) root: Option<String>,
}

pub(super) fn xml_framing(source: &str) -> XmlFraming {
    let trimmed = source.trim_start();
    let declaration = trimmed
        .starts_with("<?xml")
        .then(|| trimmed.find("?>").map(|end| trimmed[..end + 2].to_string()))
        .flatten();
    let tokens = tokenize(source);
    let root = plan_paths(source, &tokens).root.and_then(|start| {
        tokens
            .iter()
            .find(|token| token.start == start)
            .map(|token| token_name(source, token))
    });
    XmlFraming { declaration, root }
}

/// Writes several elements for an array, with two-space indent.
pub(super) fn write_element(out: &mut String, name: &str, value: &Value) -> Result<(), String> {
    use serde::Serialize;
    let items: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    for item in items {
        let mut serializer = quick_xml::se::Serializer::with_root(&mut *out, Some(name))
            .map_err(|error| format!("<{name}>: {error}"))?;
        serializer.indent(' ', 2);
        with_serializer_text(item)
            .serialize(serializer)
            .map_err(|error| format!("<{name}>: {error}"))?;
        out.push('\n');
    }
    Ok(())
}

/// `value` with the model's `#text` (an element's text beside its children)
/// renamed to `$text`, the key `quick-xml` writes as element text.
fn with_serializer_text(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| {
                    let key = if key == "#text" { "$text".to_string() } else { key.clone() };
                    (key, with_serializer_text(child))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(with_serializer_text).collect()),
        other => other.clone(),
    }
}
