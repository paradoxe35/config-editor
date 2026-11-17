//! A key's hover says "required" only where its object requires it: not
//! for a key only one `anyOf` option requires ("at least one of"), however
//! that `anyOf` is reached (through `allOf`, through an outer `anyOf` or a
//! `$ref`), in every language.

use super::super::completion::key_hover;
use super::super::schema::build_schema;
use super::harness::{hovering, key_place, report, text};
use serde_json::{Value, json};

/// The dependency object: `version` or `git` (at least one), `path`.
fn dependency() -> Value {
    json!({
        "type": "object",
        "properties": {"version": {"type": "string"}, "git": {"type": "string"}, "path": {"type": "string"}},
        "allOf": [{"anyOf": [
            {"properties": {"version": true}, "required": ["version"]},
            {"properties": {"git": true}, "required": ["git"]}
        ]}]
    })
}

fn variants() -> Vec<(&'static str, Value)> {
    let deps = |each: Value| json!({"type": "object", "properties": {"deps": {"type": "object", "additionalProperties": each}}});
    let with_definitions = |mut schema: Value| {
        schema["definitions"] = json!({"D": dependency()});
        schema
    };
    let mut with_not = dependency();
    with_not["allOf"]
        .as_array_mut()
        .unwrap()
        .push(json!({"not": {"required": ["git", "path"]}}));
    vec![
        (
            "$ref",
            with_definitions(deps(json!({"$ref": "#/definitions/D"}))),
        ),
        (
            "anyOf string or $ref",
            with_definitions(deps(
                json!({"anyOf": [{"type": "string"}, {"$ref": "#/definitions/D"}]}),
            )),
        ),
        (
            "anyOf string or inline",
            deps(json!({"anyOf": [{"type": "string"}, dependency()]})),
        ),
        ("with not", deps(with_not)),
        // Required outright: said so.
        (
            "required",
            deps(
                json!({"type": "object", "required": ["version"], "properties": {"version": {"type": "string"}}}),
            ),
        ),
    ]
}

#[test]
fn hover_says_required_only_where_the_object_requires_the_key() {
    let mut failures = Vec::new();
    for (name, schema) in variants() {
        let bundle = build_schema(&schema.to_string()).unwrap();
        let wanted = name == "required";
        let document = json!({"deps": {"serde": {"version": "1"}}});
        for language in hovering() {
            let place = text(language, &document, &[]).and_then(|text| {
                key_place(language, &text, "/deps/serde/version", 2).map(|place| (text, place))
            });
            let Ok((text, (line, column))) = place else {
                failures.push(format!("{name}, {language}: {place:?}"));
                continue;
            };
            let hover = key_hover(&text, line, column, Some(&bundle), language.behavior());
            let hovered = hover.as_ref().map(|hover| hover.required);
            if hovered != Some(wanted) {
                failures.push(format!("{name}, {language}: hover required {hovered:?}"));
            }
        }
    }
    report(&failures);
}
