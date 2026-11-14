//! Completion and hover follow every kind of reference validation does: a
//! `$dynamicRef` to a `$dynamicAnchor`, a `$recursiveRef`, and a draft-04
//! `id: "#foo"` named by `$ref: "#foo"`.
//!
//! `|` marks the caret.

use super::super::completion::{completions, key_hover};
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::{Value, json};

fn schemas() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        (
            "$dynamicRef",
            json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "$id": "https://ex.com/tree",
                "$dynamicAnchor": "node", "type": "object",
                "properties": {"data": {"type": "string", "description": "payload"},
                    "kids": {"type": "array", "items": {"$dynamicRef": "#node"}}}}),
            "data",
        ),
        (
            "$recursiveRef",
            json!({"$schema": "https://json-schema.org/draft/2019-09/schema", "$recursiveAnchor": true,
                "type": "object",
                "properties": {"data": {"type": "string", "description": "payload"},
                    "kids": {"type": "array", "items": {"$recursiveRef": "#"}}}}),
            "data",
        ),
        (
            "draft-04 id",
            json!({"$schema": "http://json-schema.org/draft-04/schema#",
                "definitions": {"a": {"id": "#foo", "type": "object",
                    "properties": {"data": {"type": "integer", "description": "payload"}}}},
                "properties": {"kids": {"type": "array", "items": {"$ref": "#foo"}}}}),
            "data",
        ),
    ]
}

/// Where a key of an item of `kids` is typed, and the same item with
/// `data` written (its key hovered).
const DOCUMENTS: [(Language, &str, &str); 3] = [
    (Language::YAML, "kids:\n  - |\n", "kids:\n  - data: 1\n"),
    (
        Language::JSON,
        "{\"kids\": [{|}]}\n",
        "{\"kids\": [{\"data\": 1}]}\n",
    ),
    (Language::TOML, "[[kids]]\n|\n", "[[kids]]\ndata = 1\n"),
];

#[test]
fn completion_and_hover_follow_dynamic_recursive_and_draft_four_references() {
    let mut failures = Vec::new();
    for (name, schema, key) in schemas() {
        let bundle = build_schema(&schema.to_string()).unwrap();
        for (language, marked, written) in DOCUMENTS {
            let at = marked.find('|').unwrap();
            let source = marked.replacen('|', "", 1);
            let labels: Vec<String> =
                completions(&source, at, true, Some(&bundle), true, language.behavior())
                    .into_iter()
                    .map(|option| option.label)
                    .collect();
            if !labels.iter().any(|label| label == key) {
                failures.push(format!("{name}, {language}: offered {labels:?}"));
            }
            let line = written.lines().position(|line| line.contains(key)).unwrap();
            let column = written.lines().nth(line).unwrap().find(key).unwrap() + 2;
            let description = key_hover(written, line, column, Some(&bundle), language.behavior())
                .and_then(|hover| hover.description);
            if description.as_deref() != Some("payload") {
                failures.push(format!("{name}, {language}: hover {description:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
