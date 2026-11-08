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
        Value::String(text) => Value::String(super::super::super::codec::non_finite::written(text).to_string()),
        // As JSON writes it, which the reader takes back as that number
        // (`quick-xml` writes the float `1.0` as `1`, an integer).
        Value::Number(number) => Value::String(number.to_string()),
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
    let mut written_open = AttributePaths::new();
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
                attribute.push(plan.attribute_key(token.start, &source[inner.start..inner.end]));
                found.insert(attribute);
                any = true;
            }
        }
        let mut element = keys;
        element.push(ATTRIBUTES_ONLY.to_string());
        if matches!(token.kind, Kind::OpenTag { closing: false, .. }) && !source[..token.end].ends_with("/>") {
            written_open.insert(element);
        } else if any {
            found.insert(element);
        }
    }
    // A repeated element some of whose items hold children or text
    // (`<s n="a"><p>1</p></s>` beside `<s n="c"/>`) is not attributes-only.
    found.retain(|path| !written_open.contains(path));
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
                // An attribute keyed `@name` (a child element has its name).
                if scalar && key.starts_with('@') {
                    attrs.insert(key.clone(), child.clone());
                } else if scalar && (attributes_only || attributes.contains(path.as_slice())) {
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

/// `source` with the element at `pointer` renamed in its start and end tags;
/// a repeated element (a list, `pointer` naming it without an index) is
/// renamed in every repetition.
pub(super) fn rename_element(source: &str, pointer: &str, new_key: &str) -> Option<String> {
    let tokens = tokenize(source);
    let wanted = pointer_segments(pointer);
    let plan = plan_paths(source, &tokens);
    let mut names: Vec<(usize, usize)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let matches = matches!(token.kind, Kind::OpenTag { .. })
            && plan.path_of(token.start).is_some_and(|path| {
                let path = strings(path);
                path == wanted
                    || path.split_last().is_some_and(|(last, rest)| last.parse::<usize>().is_ok() && rest == wanted.as_slice())
            });
        if !matches {
            continue;
        }
        let Some((open, close)) = element_from(source, &tokens, index) else {
            continue;
        };
        let start = local_name_start(source, &tokens[open]);
        names.push((start, start + token_name(source, &tokens[open]).len()));
        if let Some(close) = close {
            let token = &tokens[close];
            let name = token_name(source, token);
            let at = local_name_start(source, token);
            names.push((at, at + name.len()));
        }
    }
    if names.is_empty() {
        return None;
    }
    names.sort_unstable();
    let mut renamed = source.to_string();
    for (start, end) in names.into_iter().rev() {
        renamed.replace_range(start..end, new_key);
    }
    Some(renamed)
}

/// The start tag at token `index` and its end tag, if it has one.
pub(super) fn element_from(source: &str, tokens: &[Token], index: usize) -> Option<(usize, Option<usize>)> {
    if source[..tokens[index].end].ends_with("/>") {
        return Some((index, None));
    }
    let mut depth = 0usize;
    for (offset, token) in tokens[index + 1..].iter().enumerate() {
        match token.kind {
            Kind::OpenTag { .. } if !source[..token.end].ends_with("/>") => depth += 1,
            Kind::EndTag if depth == 0 => return Some((index, Some(index + 1 + offset))),
            Kind::EndTag => depth -= 1,
            _ => {}
        }
    }
    Some((index, None))
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

/// The children of an element on one line (`<a>1</a><b>2</b>`), when every
/// child is text or a list of text.
pub(super) fn one_line_children(value: &Value) -> Option<String> {
    let mut out = String::new();
    for (key, child) in value.as_object()? {
        let items: Vec<&Value> = match child {
            Value::Array(items) => items.iter().collect(),
            other => vec![other],
        };
        for item in items {
            if item.is_object() || item.is_array() || key.starts_with(['@', '#', '$']) {
                return None;
            }
            let text = quick_xml::escape::escape(xml_text(item)).into_owned();
            out.push_str(&format!("<{key}>{text}</{key}>"));
        }
    }
    Some(out)
}

/// The element paths (indices left out) `source` writes as an empty element
/// with an end tag (`<beta a="1"></beta>`).
fn empty_with_end_tag(source: &str) -> std::collections::HashSet<Vec<String>> {
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let mut found = std::collections::HashSet::new();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.kind, Kind::OpenTag { .. })
            && !source[..token.end].ends_with("/>")
            && tokens.get(index + 1).is_some_and(|next| matches!(next.kind, Kind::EndTag) && next.start == token.end)
            && let Some(path) = plan.path_of(token.start)
        {
            found.insert(strings(path).into_iter().filter(|segment| segment.parse::<usize>().is_err()).collect());
        }
    }
    found
}

/// `rendered` with each self-closing element `previous` writes with an end
/// tag written with one.
pub(super) fn with_end_tags_of(rendered: &str, previous: &str) -> String {
    let wanted = empty_with_end_tag(previous);
    if wanted.is_empty() {
        return rendered.to_string();
    }
    let tokens = tokenize(rendered);
    let plan = plan_paths(rendered, &tokens);
    let mut out = rendered.to_string();
    for token in tokens.iter().rev() {
        if matches!(token.kind, Kind::OpenTag { .. })
            && rendered[..token.end].ends_with("/>")
            && let Some(path) = plan.path_of(token.start)
        {
            let path: Vec<String> = strings(path).into_iter().filter(|segment| segment.parse::<usize>().is_err()).collect();
            if wanted.contains(&path) {
                let name = token_name(rendered, token);
                let mut close = token.end - 2;
                while close > token.start && rendered[..close].ends_with(' ') {
                    close -= 1;
                }
                out.replace_range(close..token.end, &format!("></{name}>"));
            }
        }
    }
    out
}
