//! An item of a list a fill writes with items (`minItems`) is filled as a
//! single value of its schema is, in every language: an object item with
//! its required children in the same order (an XML attribute first, in
//! its start tag, not a child element), an object below it as a block, a
//! scalar item its default, a list item a list of its own such items.
//!
//! `|` marks the caret.

use super::super::completion::accept_marked;
use super::super::schema::{Schema, build_schema};
use super::Language;
use super::harness::{completing, report};
use serde_json::{Value, json};

/// An object requiring an element child (`name`) and an attribute (`id`,
/// listed after it).
fn with_attribute() -> Value {
    json!({"type": "object", "required": ["name", "id"], "properties": {
        "name": {"type": "string", "default": "n"},
        "id": {"type": "string", "default": "i", "xml": {"attribute": true}}
    }})
}

/// Each item kind: its name and the item schema.
fn kinds() -> Vec<(&'static str, Value)> {
    vec![
        ("object with a required attribute", with_attribute()),
        (
            "object with a required child",
            json!({"type": "object", "required": ["limits"], "properties": {
                "limits": {"type": "object", "required": ["cpu"], "properties": {
                    "cpu": {"type": "integer", "default": 2}}}}}),
        ),
        ("scalar item", json!({"type": "integer", "default": 7})),
        (
            "nested list",
            json!({"type": "array", "minItems": 1, "items": with_attribute()}),
        ),
    ]
}

/// `svc` requiring `one`, a value of `item`, and `xs`, a list of one item
/// at least.
fn requiring(item: &Value) -> Value {
    json!({"type": "object", "properties": {"svc": {
        "type": "object", "required": ["one", "xs"], "properties": {
            "one": item,
            "xs": {"type": "array", "minItems": 1, "items": item}
        }
    }}})
}

#[test]
fn list_items_are_filled_as_a_single_value_of_their_schema_in_every_language() {
    let mut failures = Vec::new();
    for (kind, item) in kinds() {
        let schema = requiring(&item).to_string();
        let bundle = build_schema(&schema).unwrap();
        let compiled = Schema::compile(&schema).unwrap();
        for language in completing() {
            let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
            let text = accepted.replace('|', "");
            let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
            let at = |pointer: &str| {
                read.as_ref()
                    .ok()
                    .and_then(|read| read.pointer(pointer))
                    .cloned()
            };
            let (one, xs) = (at("/svc/one"), at("/svc/xs"));
            let problems: Vec<String> = compiled
                .validate(language, &text)
                .into_iter()
                .map(|problem| problem.message)
                .collect();
            // The item reads as the single value, its keys in its order.
            let same = one.is_some()
                && xs.as_ref().map(Value::to_string)
                    == one
                        .as_ref()
                        .map(|one| Value::Array(vec![one.clone()]).to_string());
            // An attribute is written in a start tag, never as an element.
            let elements = language == Language::XML && text.contains("<id>");
            if !same || !problems.is_empty() || elements {
                failures.push(format!(
                    "{language}, {kind}:\n{accepted}\none {one:?}\nxs {xs:?}\n{problems:?}"
                ));
            }
        }
    }
    report(&failures);
}
