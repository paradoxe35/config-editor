//! A required list an accepted key writes with its least items is valid
//! the moment it is written, in every language: distinct items where it
//! asks `uniqueItems` (an enum's members, numbers, text), all the items its
//! `minItems` asks, and the first ones matching its `contains`.

use super::super::completion::accept_marked;
use super::super::schema::{Schema, build_schema};
use super::Language;
use serde_json::{Value, json};

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// `svc` requiring only the list `xs` of schema `list`.
fn requiring(list: Value) -> Value {
    json!({"type": "object", "properties": {"svc": {
        "type": "object", "required": ["xs"], "properties": {"xs": list}
    }}})
}

/// Each list schema with the items written for it.
fn cases() -> Vec<(Value, Value)> {
    vec![
        (
            json!({"type": "array", "minItems": 2, "uniqueItems": true, "items": {"enum": ["r", "w"]}}),
            json!(["r", "w"]),
        ),
        (
            json!({"type": "array", "minItems": 3, "uniqueItems": true, "items": {"type": "integer"}}),
            json!([0, 1, 2]),
        ),
        (
            json!({"type": "array", "minItems": 2, "uniqueItems": true,
                   "items": {"type": "integer", "minimum": 5}}),
            json!([5, 6]),
        ),
        (
            json!({"type": "array", "minItems": 2, "uniqueItems": true, "items": {"type": "string", "default": "x"}}),
            json!(["x", "x2"]),
        ),
        (
            json!({"type": "array", "minItems": 10, "items": {"type": "integer"}}),
            json!([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ),
        (
            json!({"type": "array", "minItems": 1, "contains": {"const": "must"}, "items": {"type": "string"}}),
            json!(["must"]),
        ),
        (
            json!({"type": "array", "minItems": 3, "uniqueItems": true, "minContains": 2,
                   "contains": {"type": "integer", "minimum": 7}, "items": {"type": "integer"}}),
            json!([7, 8, 0]),
        ),
        (
            json!({"type": "array", "minItems": 2, "uniqueItems": true, "items": {"type": "object",
                   "required": ["name"], "properties": {"name": {"type": "string"}}}}),
            json!([{"name": ""}, {"name": "a"}]),
        ),
        (
            json!({"type": "array", "minItems": 2, "uniqueItems": true, "items": {"type": "array",
                   "minItems": 1, "items": {"type": "integer"}}}),
            json!([[0], [1]]),
        ),
        (
            json!({"type": "array", "minItems": 3, "uniqueItems": true, "items": {"type": "string", "minLength": 2}}),
            json!(["aa", "ab", "ac"]),
        ),
        (
            json!({"type": "array", "minItems": 2, "uniqueItems": true,
                   "items": {"type": "string", "pattern": "^[A-Z]{2}$"}}),
            json!(["AA", "AB"]),
        ),
        (
            json!({"type": "array", "minItems": 2, "items": {"type": "string", "pattern": "^[0-9]+$"}}),
            json!(["0", "0"]),
        ),
        (
            json!({"type": "array", "minItems": 3, "contains": {"const": "m"}, "maxContains": 1,
                   "items": {"enum": ["m", "n"]}}),
            json!(["m", "n", "n"]),
        ),
        (
            json!({"type": "array", "minItems": 3, "uniqueItems": true,
                   "items": {"type": "integer", "multipleOf": 1000, "minimum": 1}}),
            json!([1000, 2000, 3000]),
        ),
        (
            json!({"type": "array", "minItems": 3, "uniqueItems": true,
                   "items": {"type": "number", "multipleOf": 0.5, "exclusiveMinimum": 0, "exclusiveMaximum": 2}}),
            json!([0.5, 1, 1.5]),
        ),
    ]
}

#[test]
fn a_least_list_is_valid_the_moment_it_is_written() {
    let mut failures = Vec::new();
    for (list, wanted) in cases() {
        let schema = requiring(list.clone()).to_string();
        let bundle = build_schema(&schema).unwrap();
        let compiled = Schema::compile(&schema).unwrap();
        for language in LANGUAGES {
            let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
            let text = accepted.replace('|', "");
            let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
            let found = read
                .as_ref()
                .ok()
                .and_then(|read| read.pointer("/svc/xs"))
                .cloned();
            let problems: Vec<String> = compiled
                .validate(language, &text)
                .into_iter()
                .map(|problem| problem.message)
                .collect();
            // Written in the document's own style: no JSON-quoted keys
            // where the language writes them bare.
            let quoted_key = language != Language::JSON && accepted.contains("\"name\"");
            if found.as_ref() != Some(&wanted) || !problems.is_empty() || quoted_key {
                failures.push(format!(
                    "{language} {list}\n{accepted}\n{found:?} {problems:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The items one fill writes are bounded, so a schema asking a huge list
/// (or lists of lists) writes no document too large to edit: such a list
/// is written with as many items as the fill may write, its key kept in
/// every language (an XML list with no item would be no element at all).
#[test]
fn a_huge_least_list_is_written_up_to_the_bound() {
    let lists = [
        (
            json!({"type": "array", "minItems": 300, "items": {"type": "integer"}}),
            256,
        ),
        (
            json!({"type": "array", "minItems": 1_000_000_000, "items": {"type": "integer"}}),
            256,
        ),
        (
            json!({"type": "array", "minItems": 20, "items": {"type": "object", "required": ["ys"],
                "properties": {"ys": {"type": "array", "minItems": 20, "items": {"type": "integer"}}}}}),
            20,
        ),
    ];
    let mut failures = Vec::new();
    for (list, items) in lists {
        let schema = requiring(list.clone()).to_string();
        let bundle = build_schema(&schema).unwrap();
        for language in LANGUAGES {
            let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
            let text = accepted.replace('|', "");
            let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
            let found = read
                .as_ref()
                .ok()
                .and_then(|read| read.pointer("/svc/xs"))
                .and_then(Value::as_array);
            // A list inside the last item, past the bound, still has an item.
            let inner_kept = found
                .and_then(|found| found.last())
                .and_then(|last| last.get("ys"))
                .is_none_or(|inner| inner.as_array().is_some_and(|inner| !inner.is_empty()));
            let found = found.map(Vec::len);
            if found != Some(items) || !inner_kept || accepted.len() > 16_384 {
                failures.push(format!(
                    "{language} {list}: {found:?} items, {} bytes",
                    accepted.len()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
