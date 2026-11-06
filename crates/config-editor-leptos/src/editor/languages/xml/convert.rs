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

/// The values `source` writes as attributes: each element's key path (array
/// indices left out) with the attribute's name. A rewrite writes them as
/// attributes again, so a structural save can patch the host line by line.
/// An element written with attributes only (`<acme secret="x"/>`) is in it
/// too, with `/>` as its last segment: its new scalars become attributes.
pub(super) type AttributePaths = std::collections::HashSet<Vec<String>>;

/// The segment marking an element written with attributes only.
const ATTRIBUTES_ONLY: &str = "/>";

pub(super) fn attribute_paths(source: &str) -> AttributePaths {
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let mut found = AttributePaths::new();
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { .. }) || plan.root == Some(token.start) {
            continue;
        }
        let Some(path) = plan.path_of(token.start) else {
            continue;
        };
        let keys: Vec<String> = path
            .iter()
            .filter_map(|segment| match segment {
                Seg::Key(key) => Some(key.clone()),
                Seg::Index(_) => None,
            })
            .collect();
        let mut any = false;
        for inner in tokenize_tag_inside(source, token) {
            if matches!(inner.kind, Kind::Text { quoted: false }) && is_attr_name(source, token, inner) {
                let mut attribute = keys.clone();
                attribute.push(source[inner.start..inner.end].to_string());
                found.insert(attribute);
                any = true;
            }
        }
        if any && source[..token.end].ends_with("/>") {
            let mut element = keys;
            element.push(ATTRIBUTES_ONLY.to_string());
            found.insert(element);
        }
    }
    found
}

/// `value` (the element at `path`) with the scalars the host wrote as
/// attributes renamed `@name`, the key `quick-xml` writes as an attribute,
/// and placed before the children.
pub(super) fn with_attributes(value: &Value, attributes: &AttributePaths, path: &mut Vec<String>) -> Value {
    match value {
        Value::Object(map) => {
            path.push(ATTRIBUTES_ONLY.to_string());
            let attributes_only = attributes.contains(path.as_slice());
            path.pop();
            let mut attrs = serde_json::Map::new();
            let mut children = serde_json::Map::new();
            for (key, child) in map {
                path.push(key.clone());
                let scalar = matches!(child, Value::String(_) | Value::Number(_) | Value::Bool(_));
                if scalar && (attributes_only || attributes.contains(path.as_slice())) {
                    attrs.insert(format!("@{key}"), child.clone());
                } else {
                    children.insert(key.clone(), with_attributes(child, attributes, path));
                }
                path.pop();
            }
            attrs.extend(children);
            Value::Object(attrs)
        }
        Value::Array(items) => Value::Array(
            items.iter().map(|item| with_attributes(item, attributes, path)).collect(),
        ),
        other => other.clone(),
    }
}

/// The range between the tags of the element at `pointer` when it has child
/// elements and is written on one line
/// (`<heartbeat><interval_secs>15</interval_secs></heartbeat>`).
pub(super) fn one_line_element(source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
    let wanted = pointer_segments(pointer);
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let (index, open) = tokens.iter().enumerate().find(|(_, token)| {
        matches!(token.kind, Kind::OpenTag { .. })
            && !source[..token.end].ends_with("/>")
            && plan.path_of(token.start).is_some_and(|path| strings(path) == wanted)
    })?;
    let name = token_name(source, open);
    let mut depth = 0usize;
    let mut children = false;
    for token in &tokens[index + 1..] {
        match token.kind {
            Kind::OpenTag { .. } if !source[..token.end].ends_with("/>") => {
                depth += 1;
                children = true;
            }
            Kind::OpenTag { .. } => children = true,
            Kind::EndTag if depth == 0 => {
                let inner = open.end..token.start;
                let one_line = !source[open.start..token.end].contains('\n');
                return (children && one_line && token_name(source, token) == name).then_some(inner);
            }
            Kind::EndTag => depth -= 1,
            _ => {}
        }
    }
    None
}
