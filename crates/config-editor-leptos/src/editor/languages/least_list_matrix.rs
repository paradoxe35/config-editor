//! A required list an accepted key writes with its least items is valid
//! the moment it is written, in every language: distinct items where it
//! asks `uniqueItems` (an enum's members, numbers, text), all the items its
//! `minItems` asks, and the first ones matching its `contains`.

use super::super::completion::accept_marked;
use super::super::diagnostics::{Schema, build_schema};
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
            if found.as_ref() != Some(&wanted) || !problems.is_empty() {
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
/// is written empty, and its error names the items it asks.
#[test]
fn a_huge_least_list_is_written_empty() {
    let lists = [
        json!({"type": "array", "minItems": 1_000_000, "items": {"type": "integer"}}),
        json!({"type": "array", "minItems": 20, "items":
            {"type": "array", "minItems": 20, "items": {"type": "integer"}}}),
    ];
    for list in lists {
        let schema = requiring(list).to_string();
        let bundle = build_schema(&schema).unwrap();
        for language in [Language::YAML, Language::JSON] {
            let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
            assert!(
                accepted.len() < 4096,
                "{language}: {} bytes",
                accepted.len()
            );
        }
    }
}
