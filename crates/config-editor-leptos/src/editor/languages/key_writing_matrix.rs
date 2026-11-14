//! A suggestion writes keys as the language's own writer does (the one
//! `serialize` and convert use): a key holding dots, slashes, spaces,
//! quotes, colons, non-ASCII letters, or nothing at all reads back as that
//! same key, whether it is the key accepted, a key of an object default,
//! or a required child filled in. A key the language cannot write (an XML
//! element name with a space, an HCL attribute that is no identifier) is
//! neither offered nor written, as the language's writer refuses it.
//!
//! `|` marks the caret.

use super::super::completion::{apply_completion, completions};
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::{Value, json};

const KEYS: &[&str] = &[
    "editor.fontSize",
    "app.kubernetes.io/name",
    "a b",
    "x-y",
    "\u{fc}n\u{ef}",
    "1abc",
    "true",
    "key:colon",
    "#hash",
    "@scope",
    "q\"uote",
    "it's",
    "",
];

/// Where each language takes a new key, after an entry (`a`).
fn marked(language: Language) -> &'static str {
    if language == Language::JSON {
        "{\n  \"a\": 1,\n  |\n}"
    } else if language == Language::XML {
        "<a>1</a>\n|"
    } else if language == Language::TOML || language == Language::HCL {
        "a = 1\n|"
    } else {
        "a: 1\n|"
    }
}

/// The text after accepting `label` at the caret of `marked`, or `None`
/// when it is not offered.
fn accepted(language: Language, schema: &Value, label: &str) -> Option<String> {
    let bundle = build_schema(&schema.to_string()).unwrap();
    let marked = marked(language);
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let options = completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&bundle),
        true,
        language.behavior(),
    );
    let option = options.iter().find(|option| option.label == label)?;
    Some(apply_completion(&source, option).0)
}

/// Whether `language`'s writer writes the key `key` in `document` (at
/// `pointer`) as text reading back the same.
fn writes(language: Language, document: &Value, pointer: &str) -> bool {
    let schema = json!({"type": "object", "properties": {
        "o": {"type": "object", "additionalProperties": {"type": "integer"}}
    }, "additionalProperties": {"type": "integer"}});
    language
        .serialize(document)
        .ok()
        .and_then(|text| read(language, &text, &schema, pointer))
        == Some(json!(1))
}

/// The value at `pointer` of `text` read as the schema says.
fn read(language: Language, text: &str, schema: &Value, pointer: &str) -> Option<Value> {
    let bundle = build_schema(&schema.to_string()).unwrap();
    super::super::codec::read(language.behavior(), text, Some(&bundle.schema))
        .ok()?
        .pointer(pointer)
        .cloned()
}

fn pointer_of(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

#[test]
fn awkward_keys_read_back_as_written_in_every_language() {
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        for key in KEYS {
            // Where the writer writes it: at the top, and in an object.
            let at_top = writes(language, &json!({ "a": 1, *key: 1 }), &pointer_of(key));
            let held = writes(
                language,
                &json!({ "a": 1, "o": { *key: 1 } }),
                &format!("/o{}", pointer_of(key)),
            );
            // The key itself, with its default.
            let schema = json!({"type": "object", "properties": {
                "a": {"type": "integer"},
                *key: {"type": "integer", "default": 3}
            }});
            match accepted(language, &schema, key) {
                Some(text) => {
                    let found = read(language, &text, &schema, &pointer_of(key));
                    if found != Some(json!(3)) {
                        failures.push(format!("{language} key {key:?}: {text:?} reads {found:?}"));
                    }
                }
                None if at_top => failures.push(format!("{language} key {key:?}: not offered")),
                None => {}
            }
            // An object default holding the key.
            let schema = json!({"type": "object", "properties": {
                "a": {"type": "integer"},
                "o": {"type": "object", "default": {*key: 1}}
            }});
            let text = accepted(language, &schema, "o").unwrap_or_default();
            let found = read(language, &text, &schema, &format!("/o{}", pointer_of(key)));
            if held && found != Some(json!(1)) || !held && found.is_some() {
                failures.push(format!(
                    "{language} default {key:?}: {text:?} reads {found:?}"
                ));
            }
            // A required child filled in.
            let schema = json!({"type": "object", "properties": {
                "a": {"type": "integer"},
                "o": {"type": "object", "required": [key],
                      "properties": {*key: {"type": "integer", "default": 1}}}
            }});
            let text = accepted(language, &schema, "o").unwrap_or_default();
            let found = read(language, &text, &schema, &format!("/o{}", pointer_of(key)));
            if held && found != Some(json!(1)) || !held && found.is_some() {
                failures.push(format!(
                    "{language} required {key:?}: {text:?} reads {found:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
