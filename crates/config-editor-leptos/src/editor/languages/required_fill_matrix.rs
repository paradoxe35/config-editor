//! Accepting a key whose object requires children writes them, in every
//! language alike: a required object's own required children with their
//! defaults (`limits: {cpu: 2, mem: 1G}`, not `limits: {}`), and a required
//! array as an empty array (not a YAML `tags:`, which reads as null).
//!
//! `|` marks the caret.

use super::super::completion::accept_marked;
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::{Value, json};

const SCHEMA: &str = r#"{"type":"object","properties":{"svc":{"type":"object",
    "required":["name","limits","tags","port"],
    "properties":{
        "name":{"type":"string"},
        "limits":{"type":"object","required":["cpu","mem"],"properties":{
            "cpu":{"type":"integer","default":2},
            "mem":{"type":"string","default":"1G"}}},
        "tags":{"type":"array","items":{"type":"string"}},
        "port":{"type":"integer","default":80}
    }}}}"#;

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

#[test]
fn required_children_are_filled_alike_in_every_language() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
        let text = accepted.replace('|', "");
        let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
        let svc = read.as_ref().ok().and_then(|read| read.get("svc"));
        let field = |key: &str| svc.and_then(|svc| svc.get(key)).cloned();
        let found = (field("limits"), field("tags"), field("port"));
        // XML has no empty list (an empty element reads as one empty
        // item): the list is left out, not written as another value.
        let tags = (language != Language::XML).then(|| Value::Array(Vec::new()));
        let wanted = (Some(json!({"cpu": 2, "mem": "1G"})), tags, Some(json!(80)));
        if found != wanted {
            failures.push(format!("{language}\n{accepted}\nreads {read:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The orders `svc` requires its children in.
const ORDERS: &[&[&str]] = &[
    &["limits", "tags"],
    &["name", "limits", "tags"],
    &["limits", "name", "tags"],
    &["tags", "name"],
    &["opts", "tags", "name"],
    &["ports", "name"],
];

/// Whichever child comes first, every language writes the same: an
/// object's required children with their defaults, a list as `[]` (left
/// out where the language has none), a list of objects too, and the caret
/// on the first value left to type.
#[test]
fn required_children_are_filled_alike_in_any_order() {
    let mut failures = Vec::new();
    for order in ORDERS {
        let schema = json!({"type": "object", "properties": {"svc": {
            "type": "object",
            "required": order,
            "properties": {
                "name": {"type": "string"},
                "limits": {"type": "object", "required": ["cpu"],
                           "properties": {"cpu": {"type": "integer", "default": 2}}},
                "tags": {"type": "array", "items": {"type": "string"}},
                "opts": {"type": "object", "required": ["x"],
                         "properties": {"x": {"type": "boolean", "default": true}}},
                "ports": {"type": "array", "items": {"type": "object", "required": ["p"],
                          "properties": {"p": {"type": "integer", "default": 80}}}}
            }
        }}});
        let bundle = build_schema(&schema.to_string()).unwrap();
        for language in LANGUAGES {
            let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
            let text = accepted.replace('|', "");
            let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
            let Ok(read) = read else {
                failures.push(format!(
                    "{language} {order:?}\n{accepted}\ndoes not read: {read:?}"
                ));
                continue;
            };
            let empty_list = (language != Language::XML).then(|| json!([]));
            for key in *order {
                let wanted = match *key {
                    "limits" => Some(json!({"cpu": 2})),
                    "opts" => Some(json!({"x": true})),
                    "tags" | "ports" => empty_list.clone(),
                    _ => continue,
                };
                let found = read.pointer(&format!("/svc/{key}")).cloned();
                if found != wanted {
                    failures.push(format!(
                        "{language} {order:?} {key}\n{accepted}\nreads {found:?}"
                    ));
                }
            }
            // The caret on the name's value, where there is one.
            let caret_line = accepted
                .lines()
                .find(|line| line.contains('|'))
                .unwrap_or("");
            if order.contains(&"name") && !caret_line.contains("name") {
                failures.push(format!(
                    "{language} {order:?}: the caret is not on the name\n{accepted}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A required list with `minItems` is written with that many items (each
/// filled as a required value is), not as `[]`, an error the moment it is
/// written: a list of text, of numbers with a default, of objects with
/// required children, in every language.
#[test]
fn a_required_list_with_min_items_is_written_with_its_items() {
    let schema = json!({"type": "object", "properties": {"svc": {
        "type": "object",
        "required": ["name", "env", "ports", "containers"],
        "properties": {
            "name": {"type": "string"},
            "env": {"type": "array", "minItems": 1, "items": {"type": "string"}},
            "ports": {"type": "array", "minItems": 2, "items": {"type": "integer", "default": 80}},
            "containers": {"type": "array", "minItems": 1, "items": {
                "type": "object", "required": ["image"],
                "properties": {"image": {"type": "string", "default": "app"}}}}
        }
    }}});
    let bundle = build_schema(&schema.to_string()).unwrap();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
        let text = accepted.replace('|', "");
        let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
        let field = |key: &str| {
            read.as_ref()
                .ok()
                .and_then(|read| read.pointer(&format!("/svc/{key}")))
                .cloned()
        };
        let found = (field("env"), field("ports"), field("containers"));
        let wanted = (
            Some(json!([""])),
            Some(json!([80, 80])),
            Some(json!([{"image": "app"}])),
        );
        if found != wanted {
            failures.push(format!("{language}\n{accepted}\nreads {read:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
