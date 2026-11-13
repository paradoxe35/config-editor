//! A suggestion writes a schema value (a default, a const, a required
//! child's fill) only as text the language reads back equal; a value the
//! language cannot hold (a null in TOML or XML) is left out and the caret
//! stays where the value goes. In every built-in language.

use crate::editor::Language;
use crate::editor::diagnostics::Schema;
use serde_json::{Value, json};

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// An empty `root` object in `language`, with the caret (`|`) inside it.
fn empty_root(language: Language) -> (String, usize) {
    let text = if language == Language::XML {
        "<root>\n  |\n</root>\n"
    } else if language == Language::JSON {
        "{\n  \"root\": {\n    |\n  }\n}\n"
    } else if language == Language::TOML {
        "[root]\n|\n"
    } else if language == Language::HCL {
        "root {\n  |\n}\n"
    } else {
        "root:\n  |\n"
    };
    let caret = text.find('|').unwrap();
    (text.replace('|', ""), caret)
}

/// The document after accepting the suggestion `key` in an empty `root`
/// of `language`, and the value it holds at `/root/<key>` as the schema
/// reads it (`None` where the text does not read yet: a value to type).
fn accepted(schema: &Schema, language: Language, key: &str) -> (String, Option<Value>) {
    let (source, caret) = empty_root(language);
    let offered = schema.suggestions(language, &source, caret);
    let suggestion = offered
        .iter()
        .find(|suggestion| suggestion.label() == key)
        .unwrap_or_else(|| panic!("{language}: {key} offered in {offered:?}"));
    let (text, _) = suggestion.accept(&source);
    let value = schema
        .convert(language, &text, Language::JSON)
        .ok()
        .and_then(|conversion| serde_json::from_str::<Value>(&conversion.text).ok())
        .and_then(|value| value.pointer(&format!("/root/{key}")).cloned());
    (text, value)
}

fn schema_of(property: Value) -> Schema {
    Schema::compile(
        &json!({"properties": {"root": {"type": "object", "properties": {"z": property}}}})
            .to_string(),
    )
    .unwrap()
}

/// What no default stands for: the empty value (or item) the user fills.
fn blank(value: &Option<Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.is_empty(),
        // An element or block written once, empty, is one empty item.
        Some(Value::Array(items)) => match &items[..] {
            [] => true,
            [only] => blank(&Some(only.clone())),
            _ => false,
        },
        Some(Value::Object(members)) => members.is_empty(),
        Some(_) => false,
    }
}

/// S.defaults-3 (TOML null), S.defaults-4 (XML object), S.defaults-5 (XML
/// list), S.defaults-6 (XML null): every default is written as itself, or
/// not at all where the language has no spelling for it.
#[test]
fn a_default_is_written_as_itself_or_not_at_all() {
    let cases = [
        (json!({"type": ["string", "null"], "default": null}), "null"),
        (
            json!({"type": "object", "default": {"a": 1, "b": "x"}}),
            "object",
        ),
        (
            json!({"type": "object", "default": {"a": {"b": [1, 2]}, "c": "y"}}),
            "nested object",
        ),
        (
            json!({"type": "array", "items": {"type": "integer"}, "default": [1, 2]}),
            "list",
        ),
        (
            json!({"type": "array", "items": {"type": "object"}, "default": [{"a": 1}, {"a": 2}]}),
            "list of objects",
        ),
        (
            json!({"type": "array", "default": [null]}),
            "list holding null",
        ),
        (
            json!({"type": "object", "default": {"a": null}}),
            "object holding null",
        ),
        (json!({"type": "string", "default": "a&<\"b"}), "string"),
        (json!({"type": "number", "default": 1.5}), "number"),
        (json!({"type": "integer", "default": 7}), "integer"),
        (json!({"type": "boolean", "default": true}), "boolean"),
    ];
    // A null has no spelling in TOML, nor in XML (an empty element is "").
    let holds = |language: Language, default: &Value| {
        let has_null = default.is_null()
            || default
                .as_array()
                .is_some_and(|items| items.contains(&Value::Null))
            || default
                .as_object()
                .is_some_and(|members| members.values().any(Value::is_null));
        !(has_null && (language == Language::TOML || language == Language::XML))
    };
    let mut failures = Vec::new();
    for (property, name) in cases {
        let default = property["default"].clone();
        let schema = schema_of(property);
        for language in LANGUAGES {
            let (text, read) = accepted(&schema, language, "z");
            let fine = if holds(language, &default) {
                read.as_ref() == Some(&default)
            } else {
                blank(&read)
            };
            if !fine {
                failures.push(format!("{language} {name}: {text:?} reads {read:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// S.defaults-11: a required child past the caret's is filled the same way
/// in every language: its const, its first enum value.
#[test]
fn required_children_are_filled_alike_in_every_language() {
    let schema = schema_of(json!({
        "type": "object",
        "required": ["a", "b", "c"],
        "properties": {
            "a": {"type": "string"},
            "b": {"enum": ["x", "y"]},
            "c": {"const": "k"}
        }
    }));
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let (text, read) = accepted(&schema, language, "z");
        let read = read.unwrap_or_default();
        if read.get("b") != Some(&json!("x")) || read.get("c") != Some(&json!("k")) {
            failures.push(format!("{language}: {text:?} reads {read}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A required child whose default the language cannot hold gets none: a
/// TOML or XML block writes no `nan` or `null` text for it.
#[test]
fn a_required_child_gets_no_default_the_language_cannot_hold() {
    let schema = schema_of(json!({
        "type": "object",
        "required": ["a", "n"],
        "properties": {
            "a": {"type": "string"},
            "n": {"type": ["string", "null"], "default": null}
        }
    }));
    for language in [Language::TOML, Language::XML] {
        let (text, read) = accepted(&schema, language, "z");
        let read = read.unwrap_or_default();
        assert!(
            blank(&read.get("n").cloned()),
            "{language}: {text:?} reads {read}"
        );
    }
}
