//! Completion and hover read the schema the way validation applies it:
//! conditionals for the value at hand, dependent schemas, unevaluated
//! properties, draft-07 tuples and `$ref` siblings, references by `$anchor`
//! and `$id`, and reference chains of any length. Each in both drafts where
//! the keyword exists in both.

use crate::editor::Language;
use crate::editor::diagnostics::Schema;
use crate::editor::paths::TextPosition;
use serde_json::{Value, json};

const DRAFT_07: &str = "http://json-schema.org/draft-07/schema#";
const DRAFT_2020: &str = "https://json-schema.org/draft/2020-12/schema";

fn compiled(dialect: &str, mut schema: Value) -> Schema {
    schema["$schema"] = json!(dialect);
    Schema::compile(&schema.to_string()).unwrap()
}

/// The keys offered on a new line at the end of `text` (`|` marks another
/// caret).
fn offered(schema: &Schema, text: &str) -> Vec<String> {
    let caret = text.find('|').unwrap_or(text.len());
    let text = text.replace('|', "");
    let mut labels: Vec<String> = schema
        .suggestions(Language::YAML, &text, caret)
        .iter()
        .filter(|suggestion| !suggestion.is_value())
        .map(|suggestion| suggestion.label().to_string())
        .collect();
    labels.sort();
    labels
}

/// The hover of the key starting line `line` (1-based) after its indent.
fn hover(schema: &Schema, text: &str, line: usize) -> (String, Option<String>, Option<String>) {
    let indent = text.lines().nth(line - 1).unwrap().len()
        - text.lines().nth(line - 1).unwrap().trim_start().len();
    let docs = schema
        .hover(
            Language::YAML,
            text,
            TextPosition {
                line,
                column: indent + 1,
            },
        )
        .unwrap_or_else(|| panic!("a hover on line {line} of {text:?}"));
    (docs.type_label, docs.title, docs.description)
}

fn problems(schema: &Schema, text: &str) -> usize {
    schema.validate(Language::YAML, text).len()
}

/// S.combinators-1: the `then` or `else` the object matches.
#[test]
fn a_conditional_offers_and_documents_the_branch_the_object_takes() {
    for dialect in [DRAFT_07, DRAFT_2020] {
        let schema = compiled(
            dialect,
            json!({
                "type": "object",
                "properties": {"kind": {"enum": ["a", "b"]}, "base": {"type": "string"}},
                "if": {"properties": {"kind": {"const": "a"}}, "required": ["kind"]},
                "then": {"properties": {"alpha": {"type": "string", "description": "then docs"}}},
                "else": {"properties": {"beta": {"type": "number", "description": "else docs"}}}
            }),
        );
        assert_eq!(
            offered(&schema, "kind: a\n"),
            ["alpha", "base"],
            "{dialect}"
        );
        assert_eq!(offered(&schema, "kind: b\n"), ["base", "beta"], "{dialect}");
        let then = "kind: a\nalpha: x\n";
        assert_eq!(
            hover(&schema, then, 2),
            ("string".into(), None, Some("then docs".into())),
            "{dialect}"
        );
        let otherwise = "kind: b\nbeta: 1\n";
        assert_eq!(
            hover(&schema, otherwise, 2),
            ("number".into(), None, Some("else docs".into())),
            "{dialect}"
        );
        // What validation applies.
        assert_eq!(problems(&schema, "kind: a\nalpha: 1\n"), 1, "{dialect}");
        assert_eq!(problems(&schema, "kind: b\nbeta: x\n"), 1, "{dialect}");
    }
}

/// S.combinators-2: a dependent schema, once its key is there
/// (`dependentSchemas`, draft-07 `dependencies`).
#[test]
fn a_dependent_schema_applies_once_its_key_is_written() {
    for (dialect, keyword) in [(DRAFT_07, "dependencies"), (DRAFT_2020, "dependentSchemas")] {
        let schema = compiled(
            dialect,
            json!({
                "type": "object",
                "properties": {"base": {"type": "string"}, "other": {"type": "string"}},
                keyword: {"base": {"properties": {"gamma": {"type": "string", "description": "dep docs"}}}}
            }),
        );
        assert_eq!(
            offered(&schema, "base: x\n"),
            ["gamma", "other"],
            "{dialect}"
        );
        assert_eq!(offered(&schema, "other: x\n"), ["base"], "{dialect}");
        assert_eq!(
            hover(&schema, "base: x\ngamma: y\n", 2),
            ("string".into(), None, Some("dep docs".into())),
            "{dialect}"
        );
        assert_eq!(problems(&schema, "base: x\ngamma: 1\n"), 1, "{dialect}");
    }
}

/// Design review item 3: `unevaluatedProperties` documents and offers the
/// keys it allows, and a closed object still offers its branches' keys.
#[test]
fn unevaluated_properties_document_the_keys_they_allow() {
    let schema = compiled(
        DRAFT_2020,
        json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "unevaluatedProperties": {"type": "integer", "description": "extra"}
        }),
    );
    assert_eq!(
        hover(&schema, "a: x\nmore: 1\n", 2),
        ("integer".into(), None, Some("extra".into()))
    );
    let closed = compiled(
        DRAFT_2020,
        json!({
            "type": "object",
            "properties": {"kind": {"const": "a"}},
            "required": ["kind"],
            "if": {"required": ["kind"]},
            "then": {"properties": {"alpha": {"type": "string"}}},
            "unevaluatedProperties": false
        }),
    );
    assert_eq!(offered(&closed, "kind: a\n"), ["alpha"]);
    assert_eq!(problems(&closed, "kind: a\nalpha: x\n"), 0);
}

/// S.combinators-3: a draft-07 tuple (`items` as a list) completes each
/// item from its position, as 2020-12 `prefixItems` does.
#[test]
fn tuple_items_complete_from_their_position() {
    for (dialect, keyword) in [(DRAFT_07, "items"), (DRAFT_2020, "prefixItems")] {
        let schema = compiled(
            dialect,
            json!({
                "properties": {"list": {"type": "array", keyword: [
                    {"type": "object", "properties": {"s": {"type": "string"}}},
                    {"type": "object", "properties": {"t": {"type": "number"}}}
                ]}}
            }),
        );
        assert_eq!(offered(&schema, "list:\n  - |\n"), ["s"], "{dialect}");
        assert_eq!(
            offered(&schema, "list:\n  - s: x\n  - |\n"),
            ["t"],
            "{dialect}"
        );
    }
}

/// S.combinators-7: up to draft-07 the keywords beside a `$ref` are
/// ignored by validation, and so by hover and completion; from 2019-09 on
/// they apply, and both are shown.
#[test]
fn ref_siblings_count_where_the_draft_applies_them() {
    let property =
        json!({"$ref": "#/definitions/base", "type": "number", "description": "sibling docs"});
    let shape = |dialect| {
        compiled(
            dialect,
            json!({
                "definitions": {"base": {"type": "string"}},
                "properties": {"v": property}
            }),
        )
    };
    let legacy = shape(DRAFT_07);
    assert_eq!(hover(&legacy, "v: x\n", 1), ("string".into(), None, None));
    assert_eq!(problems(&legacy, "v: x\n"), 0);
    let modern = shape(DRAFT_2020);
    assert_eq!(
        hover(&modern, "v: x\n", 1),
        ("number | string".into(), None, Some("sibling docs".into()))
    );
    assert_eq!(problems(&modern, "v: x\n"), 1);
}

/// S.refs-5: a reference by `$anchor`, by the root's `$id`, into an
/// embedded resource, or by a draft-07 `$id` anchor shows its target.
#[test]
fn references_by_anchor_and_id_show_their_target() {
    let schema = compiled(
        DRAFT_2020,
        json!({
            "$id": "https://example.com/root.json",
            "$defs": {
                "x": {"$anchor": "myAnchor", "type": "integer", "title": "Anchored"},
                "y": {"type": "integer", "title": "By id"},
                "o": {"$anchor": "obj", "type": "object", "properties": {"p": {"type": "string"}}},
                "sub": {"$id": "sub.json", "$defs": {"in": {"type": "boolean", "title": "Inner"}},
                        "properties": {"q": {"$ref": "#/$defs/in"}}}
            },
            "properties": {
                "a": {"$ref": "#myAnchor"},
                "b": {"$ref": "https://example.com/root.json#/$defs/y"},
                "o": {"$ref": "#obj"},
                "s": {"$ref": "sub.json"}
            }
        }),
    );
    assert_eq!(
        hover(&schema, "a: 1\n", 1),
        ("integer".into(), Some("Anchored".into()), None)
    );
    assert_eq!(
        hover(&schema, "b: 1\n", 1),
        ("integer".into(), Some("By id".into()), None)
    );
    assert_eq!(offered(&schema, "o:\n  |\n"), ["p"]);
    assert_eq!(
        hover(&schema, "s:\n  q: true\n", 2),
        ("boolean".into(), Some("Inner".into()), None)
    );
    assert_eq!(problems(&schema, "a: x\n"), 1);
    let legacy = compiled(
        DRAFT_07,
        json!({
            "definitions": {"l": {"$id": "#legacy", "type": "string", "title": "Legacy"}},
            "properties": {"l": {"$ref": "#legacy"}}
        }),
    );
    assert_eq!(
        hover(&legacy, "l: x\n", 1),
        ("string".into(), Some("Legacy".into()), None)
    );
}

/// S.refs-6: a chain of references of any length reaches its target, and
/// a cycle ends.
#[test]
fn reference_chains_of_any_length_reach_their_target() {
    for length in [16, 200] {
        let mut definitions = serde_json::Map::new();
        for link in 0..length {
            definitions.insert(
                format!("d{link}"),
                json!({"$ref": format!("#/$defs/d{}", link + 1)}),
            );
        }
        definitions.insert(
            format!("d{length}"),
            json!({"type": "integer", "title": "Deep end"}),
        );
        let schema = compiled(
            DRAFT_2020,
            json!({"$defs": definitions, "properties": {"deep": {"$ref": "#/$defs/d0"}}}),
        );
        assert_eq!(
            hover(&schema, "deep: 1\n", 1),
            ("integer".into(), Some("Deep end".into()), None),
            "{length} links"
        );
    }
    let cycle = compiled(
        DRAFT_2020,
        json!({
            "$defs": {"a": {"$ref": "#/$defs/b"}, "b": {"$ref": "#/$defs/a", "title": "B"}},
            "properties": {"c": {"$ref": "#/$defs/a"}}
        }),
    );
    assert_eq!(hover(&cycle, "c: 1\n", 1).1.as_deref(), Some("B"));
}
