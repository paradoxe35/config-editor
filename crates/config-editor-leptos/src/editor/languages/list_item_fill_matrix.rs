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

/// `svc` requiring a list of `least` items at least, each an `item`: as
/// its own child `xs`, one level down in `deep`, and as `side`, whose
/// property says nothing of it and an `allOf` member beside it makes the
/// list.
fn asking(item: &Value, least: u64) -> Value {
    let list = json!({"type": "array", "minItems": least, "items": item});
    json!({"$defs": {"x": {"type": "string", "pattern": "^x"}},
        "type": "object", "properties": {"svc": {
        "type": "object", "required": ["xs", "deep", "side"], "properties": {
            "xs": list,
            "deep": {"type": "object", "required": ["xs"], "properties": {"xs": list}},
            "side": {"description": "d"}
        },
        "allOf": [{"properties": {"side": list}}]
    }}})
}

/// The places of [`asking`]'s lists.
const LISTS: [&str; 3] = ["/svc/xs", "/svc/deep/xs", "/svc/side"];

/// Text items of a pattern no plain letters or digits meet.
fn patterned() -> Vec<(&'static str, Value)> {
    let text = |pattern: &str| json!({"type": "string", "pattern": pattern});
    vec![
        ("text with a pattern", text("^x")),
        (
            "text with a pattern through a reference",
            json!({"$ref": "#/$defs/x"}),
        ),
        ("text with a class and a count", text("^[0-9]{2}-[A-F]+$")),
        (
            "text with a choice and an escape",
            text(r"^(alpha|beta)\.v\d$"),
        ),
    ]
}

/// A fill writes as many items as `minItems` asks (0 to 3), whatever the
/// item is, in every language, and the filled text has no problem.
#[test]
fn a_fill_writes_the_items_min_items_asks_in_every_language() {
    let mut failures = Vec::new();
    for (kind, item) in kinds().into_iter().chain(patterned()) {
        for least in 0..=3 {
            let schema = asking(&item, least).to_string();
            let bundle = build_schema(&schema).unwrap();
            let compiled = Schema::compile(&schema).unwrap();
            for language in completing() {
                let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
                let text = accepted.replace('|', "");
                let read =
                    super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
                let count = |pointer: &str| {
                    read.as_ref()
                        .ok()
                        .and_then(|read| read.pointer(pointer))
                        .and_then(Value::as_array)
                        .map(Vec::len)
                };
                let problems: Vec<String> = compiled
                    .validate(language, &text)
                    .into_iter()
                    .map(|problem| problem.message)
                    .collect();
                // XML writes no empty list (documented): a list asking no
                // item is left out, which only its `required` refuses.
                let unwritten = least == 0 && language == Language::XML;
                let enough =
                    |count: Option<usize>| count.map_or(unwritten, |count| count as u64 >= least);
                let counts = LISTS.map(count);
                if !counts.into_iter().all(enough) || (!unwritten && !problems.is_empty()) {
                    failures.push(format!(
                        "{language}, {kind}, minItems {least}:\n{accepted}\n{LISTS:?}: {counts:?}\n{problems:?}"
                    ));
                }
            }
        }
    }
    report(&failures);
}

/// A required object whose property says nothing of it is filled with
/// the required children an `allOf` member beside it gives it, in every
/// language, as one its property describes is.
#[test]
fn a_required_object_described_beside_its_property_is_filled_in_every_language() {
    let object = json!({"type": "object", "required": ["k"], "properties": {
        "k": {"type": "integer", "default": 1}}});
    let schema = json!({"type": "object", "properties": {"svc": {
        "type": "object", "required": ["one"], "properties": {"one": {"description": "d"}},
        "allOf": [{"properties": {"one": object}}]
    }}})
    .to_string();
    let bundle = build_schema(&schema).unwrap();
    let compiled = Schema::compile(&schema).unwrap();
    let mut failures = Vec::new();
    for language in completing() {
        let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
        let text = accepted.replace('|', "");
        let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
        let k = read
            .ok()
            .and_then(|read| read.pointer("/svc/one/k").cloned());
        let problems: Vec<String> = compiled
            .validate(language, &text)
            .into_iter()
            .map(|problem| problem.message)
            .collect();
        if k != Some(json!(1)) || !problems.is_empty() {
            failures.push(format!("{language}:\n{accepted}\nk {k:?}\n{problems:?}"));
        }
    }
    report(&failures);
}
