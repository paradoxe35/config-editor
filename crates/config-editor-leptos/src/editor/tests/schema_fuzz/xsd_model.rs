//! A small model of XML Schema for the XSD properties: element-only
//! content (a sequence or a choice of local elements, each with its
//! occurrences), attributes, and simple types with facets (integer and
//! decimal bounds, enumerations, a longest length). The model is written as
//! JSON (so a failing case shrinks), as an XSD text, and as documents; and
//! it says itself whether a document is valid: the reference the editor's
//! validation of the XSD is judged by.
//!
//! An element is `{"n": name, "min": 0|1, "max": 1|2|"u", "named": bool}`
//! with `"s": simple` (text content) or `"seq"`/`"alt": [elements]` and
//! `"at": [{"n", "s", "req"}]` (attributes). A simple type is
//! `{"int": [low, high]}`, `{"dec": [low, high]}`, `{"bool": 1}` or
//! `{"text": [values…], "len": most}` (no values: any letters). A document
//! is `{"n": name, "a": {attribute: text}, "c": [children], "t": text}`.

use serde_json::{Map, Value, json};

use super::random::Rng;

/// Element names (a content model's children are told apart by name).
const ELEMENTS: &[&str] = &[
    "host", "port", "mode", "item", "user", "path", "rule", "opt",
];
/// Attribute names, apart from the element names.
const ATTRIBUTES: &[&str] = &["id", "kind", "level", "on"];
/// Enumeration values.
const VALUES: &[&str] = &["tcp", "udp", "auto", "off"];

/// A random model: a root element with element-only content.
pub(super) fn model(rng: &mut Rng) -> Value {
    let most = 2 + rng.below(2);
    let mut root = complex(rng, 0, most);
    root["n"] = json!("config");
    root["min"] = json!(1);
    root["max"] = json!(1);
    root
}

fn complex(rng: &mut Rng, depth: usize, most: usize) -> Value {
    let mut names: Vec<&str> = ELEMENTS.to_vec();
    let count = 1 + rng.below(4);
    let mut children = Vec::new();
    for _ in 0..count {
        let name = names.remove(rng.below(names.len()));
        let mut child = if depth + 1 < most && rng.chance(30) {
            complex(rng, depth + 1, most)
        } else {
            json!({"s": simple(rng)})
        };
        child["n"] = json!(name);
        child["min"] = json!(usize::from(rng.chance(60)));
        child["max"] = match rng.below(6) {
            0 => json!(2),
            1 => json!("u"),
            _ => json!(1),
        };
        if rng.chance(25) {
            child["named"] = json!(true);
        }
        children.push(child);
    }
    let mut attributes: Vec<&str> = ATTRIBUTES.to_vec();
    let mut at = Vec::new();
    for _ in 0..rng.below(3) {
        let name = attributes.remove(rng.below(attributes.len()));
        at.push(json!({"n": name, "s": simple(rng), "req": rng.chance(40)}));
    }
    let group = if rng.chance(25) { "alt" } else { "seq" };
    let mut node = Map::new();
    node.insert(group.into(), Value::Array(children));
    node.insert("at".into(), Value::Array(at));
    if rng.chance(25) {
        node.insert("named".into(), json!(true));
    }
    Value::Object(node)
}

fn simple(rng: &mut Rng) -> Value {
    match rng.below(5) {
        0 => {
            let low = rng.below(5) as i64 - 2;
            json!({"int": [low, low + 1 + rng.below(100) as i64]})
        }
        1 => {
            let low = rng.below(3) as i64;
            json!({"dec": [low, low + 1 + rng.below(10) as i64]})
        }
        2 => json!({"bool": 1}),
        3 => {
            let mut values: Vec<&str> = VALUES.to_vec();
            let kept: Vec<&str> = (0..1 + rng.below(3))
                .map(|_| values.remove(rng.below(values.len())))
                .collect();
            json!({"text": kept})
        }
        _ => {
            let mut node = json!({"text": []});
            if rng.chance(50) {
                node["len"] = json!(2 + rng.below(4));
            }
            node
        }
    }
}

/// An element's children and whether they are a choice; `None` for text
/// content.
pub(super) fn children(element: &Value) -> Option<(&[Value], bool)> {
    if let Some(Value::Array(items)) = element.get("seq") {
        return Some((items, false));
    }
    if let Some(Value::Array(items)) = element.get("alt") {
        return Some((items, true));
    }
    None
}

pub(super) fn attributes(element: &Value) -> &[Value] {
    element
        .get("at")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub(super) fn name(node: &Value) -> Option<&str> {
    node.get("n").and_then(Value::as_str)
}

fn min(element: &Value) -> usize {
    element.get("min").and_then(Value::as_u64).unwrap_or(1) as usize
}

/// Most occurrences; `None` for unbounded.
pub(super) fn max(element: &Value) -> Option<usize> {
    match element.get("max") {
        Some(Value::String(_)) => None,
        Some(value) => Some(value.as_u64().unwrap_or(1) as usize),
        None => Some(1),
    }
}

/// Whether the model is well formed (a shrunk one may not be): names
/// present and apart within each content model, groups not empty, simple
/// types known, at most `max` at least `min`.
pub(super) fn well_formed(element: &Value) -> bool {
    let Some(_) = name(element) else {
        return false;
    };
    if max(element).is_some_and(|most| most < min(element).max(1)) {
        return false;
    }
    match children(element) {
        Some((items, _)) => {
            let mut seen: Vec<&str> = Vec::new();
            for item in items {
                let Some(name) = name(item) else { return false };
                if seen.contains(&name) || !well_formed(item) {
                    return false;
                }
                seen.push(name);
            }
            let mut names: Vec<&str> = Vec::new();
            for attribute in attributes(element) {
                let (Some(name), Some(kind)) = (name(attribute), attribute.get("s")) else {
                    return false;
                };
                if names.contains(&name) || seen.contains(&name) || !simple_well_formed(kind) {
                    return false;
                }
                names.push(name);
            }
            !items.is_empty()
        }
        None => element.get("s").is_some_and(simple_well_formed),
    }
}

fn simple_well_formed(kind: &Value) -> bool {
    let bounds = |key: &str| {
        kind.get(key).and_then(Value::as_array).is_some_and(|pair| {
            pair.len() == 2
                && pair[0]
                    .as_i64()
                    .zip(pair[1].as_i64())
                    .is_some_and(|(low, high)| low <= high)
        })
    };
    bounds("int")
        || bounds("dec")
        || kind.get("bool").is_some()
        || kind
            .get("text")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().all(|value| value.as_str().is_some_and(plain)))
}

/// Text the XML mapping reads as itself (letters and digits, not empty).
pub(super) fn plain(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The XSD text of `root`, with the option `root` naming it.
pub(super) fn xsd(root: &Value) -> String {
    let mut globals = Vec::new();
    let mut counter = 0;
    let body = element_decl(root, &mut globals, &mut counter, true);
    format!(
        "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n{body}\n{}</xs:schema>\n",
        globals.concat()
    )
}

fn element_decl(
    element: &Value,
    globals: &mut Vec<String>,
    counter: &mut usize,
    top: bool,
) -> String {
    let name = name(element).unwrap_or("e");
    let mut head = format!("<xs:element name=\"{name}\"");
    if !top {
        let low = min(element);
        if low != 1 {
            head.push_str(&format!(" minOccurs=\"{low}\""));
        }
        match max(element) {
            None => head.push_str(" maxOccurs=\"unbounded\""),
            Some(1) => {}
            Some(most) => head.push_str(&format!(" maxOccurs=\"{most}\"")),
        }
    }
    let named = element
        .get("named")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let definition = match children(element) {
        Some((items, choice)) => {
            let group = if choice { "choice" } else { "sequence" };
            let inner: String = items
                .iter()
                .map(|item| element_decl(item, globals, counter, false))
                .collect();
            let attributes: String = attributes(element)
                .iter()
                .map(|attribute| attribute_decl(attribute, globals, counter))
                .collect();
            (
                "complexType",
                format!("<xs:{group}>{inner}</xs:{group}>{attributes}"),
            )
        }
        None => {
            let kind = element.get("s").cloned().unwrap_or(json!({"bool": 1}));
            match plain_type(&kind) {
                Some(builtin) if !named => return format!("{head} type=\"{builtin}\"/>"),
                _ => ("simpleType", restriction(&kind)),
            }
        }
    };
    let (tag, inner) = definition;
    if named {
        *counter += 1;
        let type_name = format!("t{counter}");
        globals.push(format!(
            "<xs:{tag} name=\"{type_name}\">{inner}</xs:{tag}>\n"
        ));
        format!("{head} type=\"{type_name}\"/>")
    } else {
        format!("{head}><xs:{tag}>{inner}</xs:{tag}></xs:element>")
    }
}

fn attribute_decl(attribute: &Value, globals: &mut Vec<String>, counter: &mut usize) -> String {
    let name = name(attribute).unwrap_or("a");
    let required = attribute
        .get("req")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let usage = if required { " use=\"required\"" } else { "" };
    let kind = attribute.get("s").cloned().unwrap_or(json!({"bool": 1}));
    match plain_type(&kind) {
        Some(builtin) => format!("<xs:attribute name=\"{name}\" type=\"{builtin}\"{usage}/>"),
        None => {
            *counter += 1;
            let type_name = format!("t{counter}");
            globals.push(format!(
                "<xs:simpleType name=\"{type_name}\">{}</xs:simpleType>\n",
                restriction(&kind)
            ));
            format!("<xs:attribute name=\"{name}\" type=\"{type_name}\"{usage}/>")
        }
    }
}

/// The built-in type a simple type is, when it has no facets.
fn plain_type(kind: &Value) -> Option<&'static str> {
    if kind.get("bool").is_some() {
        return Some("xs:boolean");
    }
    let values = kind.get("text").and_then(Value::as_array)?;
    (values.is_empty() && kind.get("len").is_none()).then_some("xs:string")
}

fn restriction(kind: &Value) -> String {
    let bounds = |key: &str| {
        let pair = kind
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        format!(
            "<xs:minInclusive value=\"{}\"/><xs:maxInclusive value=\"{}\"/>",
            pair.first().cloned().unwrap_or(json!(0)),
            pair.get(1).cloned().unwrap_or(json!(0))
        )
    };
    if kind.get("int").is_some() {
        return format!(
            "<xs:restriction base=\"xs:int\">{}</xs:restriction>",
            bounds("int")
        );
    }
    if kind.get("dec").is_some() {
        return format!(
            "<xs:restriction base=\"xs:decimal\">{}</xs:restriction>",
            bounds("dec")
        );
    }
    if kind.get("bool").is_some() {
        return "<xs:restriction base=\"xs:boolean\"/>".to_string();
    }
    let values: String = kind
        .get("text")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(|value| format!("<xs:enumeration value=\"{value}\"/>"))
                .collect()
        })
        .unwrap_or_default();
    let length = kind
        .get("len")
        .and_then(Value::as_u64)
        .map(|most| format!("<xs:maxLength value=\"{most}\"/>"))
        .unwrap_or_default();
    format!("<xs:restriction base=\"xs:string\">{values}{length}</xs:restriction>")
}

/// A valid document of `element`.
pub(super) fn document(rng: &mut Rng, element: &Value) -> Value {
    let mut node = Map::new();
    node.insert("n".into(), json!(name(element).unwrap_or("e")));
    match children(element) {
        Some((items, choice)) => {
            let mut attributes = Map::new();
            for attribute in self::attributes(element) {
                let required = attribute
                    .get("req")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if required || rng.chance(50) {
                    let kind = attribute.get("s").cloned().unwrap_or_default();
                    attributes.insert(
                        name(attribute).unwrap_or("a").into(),
                        json!(value(rng, &kind)),
                    );
                }
            }
            let chosen: Vec<&Value> = if choice {
                vec![&items[rng.below(items.len())]]
            } else {
                items.iter().collect()
            };
            let mut children = Vec::new();
            for item in chosen {
                let low = min(item).max(usize::from(choice));
                let most = max(item).unwrap_or(3).min(3);
                for _ in 0..low + rng.below(most - low.min(most) + 1) {
                    children.push(document(rng, item));
                }
            }
            node.insert("a".into(), Value::Object(attributes));
            node.insert("c".into(), Value::Array(children));
        }
        None => {
            let kind = element.get("s").cloned().unwrap_or_default();
            node.insert("t".into(), json!(value(rng, &kind)));
        }
    }
    Value::Object(node)
}

/// A valid value of a simple type.
fn value(rng: &mut Rng, kind: &Value) -> String {
    let pair = |key: &str| {
        kind.get(key).and_then(Value::as_array).and_then(|pair| {
            pair.first()
                .and_then(Value::as_i64)
                .zip(pair.get(1).and_then(Value::as_i64))
        })
    };
    if let Some((low, high)) = pair("int") {
        return (low + rng.below((high - low + 1) as usize) as i64).to_string();
    }
    if let Some((low, high)) = pair("dec") {
        let tenths = low * 10 + rng.below(((high - low) * 10 + 1) as usize) as i64;
        return format!("{}.{}", tenths.div_euclid(10), tenths.rem_euclid(10));
    }
    if kind.get("bool").is_some() {
        return rng.pick(&["true", "false", "1", "0"]).to_string();
    }
    let values: Vec<&str> = kind
        .get("text")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !values.is_empty() {
        return rng.pick(&values).to_string();
    }
    let most = kind.get("len").and_then(Value::as_u64).unwrap_or(6) as usize;
    let length = 1 + rng.below(most);
    (0..length)
        .map(|_| *rng.pick(&["a", "b", "x", "z"]))
        .collect()
}

/// Whether `text` is a value of `kind`, as XSD reads it.
fn value_valid(kind: &Value, text: &str) -> bool {
    let pair = |key: &str| {
        kind.get(key).and_then(Value::as_array).and_then(|pair| {
            pair.first()
                .and_then(Value::as_i64)
                .zip(pair.get(1).and_then(Value::as_i64))
        })
    };
    if let Some((low, high)) = pair("int") {
        return text
            .parse::<i64>()
            .is_ok_and(|number| (low..=high).contains(&number));
    }
    if let Some((low, high)) = pair("dec") {
        return decimal(text).is_some_and(|number| number >= low as f64 && number <= high as f64);
    }
    if kind.get("bool").is_some() {
        return ["true", "false", "1", "0"].contains(&text);
    }
    let values: Vec<&str> = kind
        .get("text")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let most = kind
        .get("len")
        .and_then(Value::as_u64)
        .map(|most| most as usize);
    (values.is_empty() || values.contains(&text))
        && most.is_none_or(|most| text.chars().count() <= most)
}

/// An `xs:decimal` lexical value (digits, one optional point, a sign).
fn decimal(text: &str) -> Option<f64> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let ok = !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.matches('.').count() <= 1
        && digits != ".";
    if ok { text.parse().ok() } else { None }
}

/// Whether `document` is valid for `element` (the model's own verdict);
/// `None` when the model cannot tell how the XML mapping reads it (a
/// text that is not plain).
pub(super) fn valid(element: &Value, document: &Value) -> Option<bool> {
    if name(document) != name(element) {
        return Some(false);
    }
    let text = document.get("t").and_then(Value::as_str);
    if text.is_some_and(|text| !plain(text)) {
        return None;
    }
    let attributes = document.get("a").and_then(Value::as_object);
    for value in attributes.into_iter().flat_map(|map| map.values()) {
        if !value.as_str().is_some_and(plain) {
            return None;
        }
    }
    let empty = Vec::new();
    let found = document
        .get("c")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let Some((items, choice)) = children(element) else {
        // Text content: no attributes or children.
        let kind = element.get("s").cloned().unwrap_or_default();
        return Some(
            attributes.is_none_or(Map::is_empty)
                && found.is_empty()
                && text.is_some_and(|text| value_valid(&kind, text)),
        );
    };
    if text.is_some() {
        return Some(false);
    }
    let given = attributes.cloned().unwrap_or_default();
    let declared = self::attributes(element);
    for (key, value) in &given {
        let Some(attribute) = declared
            .iter()
            .find(|attribute| name(attribute) == Some(key))
        else {
            return Some(false);
        };
        let kind = attribute.get("s").cloned().unwrap_or_default();
        if !value_valid(&kind, value.as_str().unwrap_or("")) {
            return Some(false);
        }
    }
    for attribute in declared {
        let required = attribute
            .get("req")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if required && !given.contains_key(name(attribute).unwrap_or("")) {
            return Some(false);
        }
    }
    let runs = runs(found);
    if choice {
        return match runs.as_slice() {
            [] => Some(items.iter().any(|item| min(item) == 0)),
            [(name, count)] => {
                let Some(item) = items.iter().find(|item| self::name(item) == Some(*name)) else {
                    return Some(false);
                };
                if *count < min(item).max(1) || max(item).is_some_and(|most| *count > most) {
                    return Some(false);
                }
                all_valid(item, found)
            }
            _ => Some(false),
        };
    }
    // A sequence of names apart: each run matches the next declarations.
    let mut next = 0;
    let mut at = 0;
    for item in items {
        let count = match runs.get(next) {
            Some((name, count)) if Some(*name) == self::name(item) => {
                next += 1;
                *count
            }
            _ => 0,
        };
        if count < min(item) || max(item).is_some_and(|most| count > most) {
            return Some(false);
        }
        match all_valid(item, &found[at..at + count]) {
            Some(true) => {}
            other => return other,
        }
        at += count;
    }
    Some(next == runs.len())
}

fn all_valid(item: &Value, found: &[Value]) -> Option<bool> {
    let mut verdict = Some(true);
    for child in found.iter().filter(|child| name(child) == name(item)) {
        match valid(item, child) {
            Some(true) => {}
            Some(false) => return Some(false),
            None => verdict = None,
        }
    }
    verdict
}

/// The children's names in runs of the same name, with their lengths.
fn runs(found: &[Value]) -> Vec<(&str, usize)> {
    let mut runs: Vec<(&str, usize)> = Vec::new();
    for child in found {
        let name = name(child).unwrap_or("");
        match runs.last_mut() {
            Some((last, count)) if *last == name => *count += 1,
            _ => runs.push((name, 1)),
        }
    }
    runs
}

/// `document` as XML text, one element per line.
pub(super) fn xml(document: &Value) -> String {
    let mut out = String::new();
    write(document, 0, &mut out);
    out
}

fn write(node: &Value, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let name = name(node).unwrap_or("e");
    let mut head = format!("{indent}<{name}");
    if let Some(attributes) = node.get("a").and_then(Value::as_object) {
        for (key, value) in attributes {
            head.push_str(&format!(" {key}=\"{}\"", value.as_str().unwrap_or("")));
        }
    }
    let children = node
        .get("c")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty());
    match (node.get("t").and_then(Value::as_str), children) {
        (Some(text), _) => out.push_str(&format!("{head}>{text}</{name}>\n")),
        (None, Some(children)) => {
            out.push_str(&format!("{head}>\n"));
            for child in children {
                write(child, depth + 1, out);
            }
            out.push_str(&format!("{indent}</{name}>\n"));
        }
        (None, None) => out.push_str(&format!("{head}/>\n")),
    }
}

/// `document` changed in one place chosen by `pick`: a text made another
/// value, a child left out, written twice or swapped with the next, a
/// child or an attribute the model does not have added, an attribute left
/// out (or none of these). The model then says whether it is still valid.
pub(super) fn mutate(document: &Value, pick: usize) -> Value {
    let mut out = document.clone();
    let mut nodes = Vec::new();
    pointers(&out, String::new(), &mut nodes);
    if nodes.is_empty() {
        return out;
    }
    let at = nodes[(pick / 8) % nodes.len()].clone();
    let Some(node) = out.pointer_mut(&at) else {
        return out;
    };
    const WRONG: [&str; 6] = ["x", "999999", "-5", "true", "zzz", "1.5"];
    match pick % 8 {
        1 => {
            if node.get("t").is_some() {
                node["t"] = json!(WRONG[(pick / 64) % WRONG.len()]);
            } else if let Some(Value::Object(attributes)) = node.get_mut("a")
                && let Some(value) = attributes.values_mut().next()
            {
                *value = json!(WRONG[(pick / 64) % WRONG.len()]);
            }
        }
        2..=4 => {
            if let Some(Value::Array(children)) = node.get_mut("c")
                && !children.is_empty()
            {
                let index = (pick / 64) % children.len();
                match pick % 8 {
                    2 => {
                        children.remove(index);
                    }
                    3 => {
                        let copy = children[index].clone();
                        children.insert(index, copy);
                    }
                    _ if index + 1 < children.len() => children.swap(index, index + 1),
                    _ => {}
                }
            }
        }
        5 => {
            if let Some(Value::Array(children)) = node.get_mut("c") {
                children.push(json!({"n": "zz", "t": "1"}));
            }
        }
        6 => {
            if let Some(Value::Object(attributes)) = node.get_mut("a")
                && let Some(key) = attributes.keys().next().cloned()
            {
                attributes.remove(&key);
            }
        }
        7 => {
            if let Some(Value::Object(attributes)) = node.get_mut("a") {
                attributes.insert("zz".into(), json!("1"));
            }
        }
        _ => {}
    }
    out
}

/// The JSON pointer of each element in `node`, outermost first.
fn pointers(node: &Value, at: String, out: &mut Vec<String>) {
    out.push(at.clone());
    if let Some(Value::Array(children)) = node.get("c") {
        for (index, child) in children.iter().enumerate() {
            pointers(child, format!("{at}/c/{index}"), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model's own verdict on a few documents (it is the reference).
    #[test]
    fn the_model_judges_documents_as_xsd_does() {
        let root = json!({"n": "config", "min": 1, "max": 1,
            "seq": [
                {"n": "host", "min": 1, "max": 1, "s": {"text": []}},
                {"n": "port", "min": 0, "max": 2, "s": {"int": [1, 10]}},
            ],
            "at": [{"n": "id", "s": {"bool": 1}, "req": true}]});
        assert!(well_formed(&root));
        let doc = |children: Value, attributes: Value| json!({"n": "config", "a": attributes, "c": children});
        let host = json!({"n": "host", "t": "h"});
        let port = |text: &str| json!({"n": "port", "t": text});
        let id = json!({"id": "1"});
        assert_eq!(
            valid(&root, &doc(json!([host, port("2"), port("3")]), id.clone())),
            Some(true)
        );
        assert_eq!(
            valid(&root, &doc(json!([host, port("11")]), id.clone())),
            Some(false)
        );
        assert_eq!(
            valid(&root, &doc(json!([port("2"), host]), id.clone())),
            Some(false)
        );
        assert_eq!(
            valid(
                &root,
                &doc(json!([host, port("1"), port("1"), port("1")]), id.clone())
            ),
            Some(false)
        );
        assert_eq!(
            valid(&root, &doc(json!([port("2")]), id.clone())),
            Some(false)
        );
        assert_eq!(valid(&root, &doc(json!([host]), json!({}))), Some(false));
        assert_eq!(
            valid(&root, &doc(json!([host]), json!({"id": "1", "zz": "1"}))),
            Some(false)
        );
        assert_eq!(
            valid(&root, &doc(json!([host, {"n": "zz", "t": "1"}]), id)),
            Some(false)
        );
    }
}
