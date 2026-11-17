//! Completion and hover follow every kind of reference validation does: a
//! `$dynamicRef` to a `$dynamicAnchor`, a `$recursiveRef`, and a draft-04
//! `id: "#foo"` named by `$ref: "#foo"`.
//!
//! `|` marks the caret.

use super::super::completion::{completions, key_hover};
use super::super::schema::build_schema;
use super::harness::{At, completing, hovering, key_place, report, text, write};
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

#[test]
fn completion_and_hover_follow_dynamic_recursive_and_draft_four_references() {
    let mut failures = Vec::new();
    for (name, schema, key) in schemas() {
        let bundle = build_schema(&schema.to_string()).unwrap();
        let languages = completing()
            .into_iter()
            .filter(|language| hovering().contains(language));
        let forms =
            languages.flat_map(|language| [(language, &[][..]), (language, &["/kids/0"][..])]);
        for (language, one_line) in forms {
            // A key of an item of `kids` typed (the item over lines and on
            // one line), and the same item with `key` written (its key
            // hovered).
            let typed = At::NewKey {
                object: "/kids/0",
                typed: "",
            };
            let cases =
                write(language, &json!({"kids": [{}]}), one_line, typed).and_then(|typed| {
                    let written = json!({"kids": [{ key: 1 }]});
                    let text = text(language, &written, &[])?;
                    let place = key_place(language, &text, &format!("/kids/0/{key}"), 2)?;
                    Ok((typed, text, place))
                });
            let Ok((typed, written, (line, column))) = cases else {
                failures.push(format!("{name}, {language}: {cases:?}"));
                continue;
            };
            let at = typed.byte;
            let source = typed.text;
            let labels: Vec<String> =
                completions(&source, at, true, Some(&bundle), true, language.behavior())
                    .into_iter()
                    .map(|option| option.label)
                    .collect();
            if !labels.iter().any(|label| label == key) {
                failures.push(format!("{name}, {language}: offered {labels:?}"));
            }
            let description = key_hover(&written, line, column, Some(&bundle), language.behavior())
                .and_then(|hover| hover.description);
            if description.as_deref() != Some("payload") {
                failures.push(format!("{name}, {language}: hover {description:?}"));
            }
        }
    }
    report(&failures);
}
