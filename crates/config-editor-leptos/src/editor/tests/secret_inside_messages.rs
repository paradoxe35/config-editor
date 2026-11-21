//! No schema problem quotes a secret, wherever the schema marks it: the
//! secret items of a list (`writeOnly`, `x-secret` on `items`), a secret
//! field of a list's items or of a map's entries, at any depth, in every
//! kind of message (a repeated item, an `enum`, a `const`, a `not`, a
//! type, the options of a `oneOf`, a `contains`, a pattern), in every
//! language. The value is still told, each secret in it hidden; a `not`
//! refusing a secret names none of the values it refuses (they would
//! tell what the secret is).

use crate::editor::Language;
use crate::editor::schema::Schema;

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "ks": {"type": "array", "uniqueItems": true, "items": {"type": "string", "writeOnly": true}},
    "kx": {"type": "array", "uniqueItems": true, "items": {"type": "string", "x-secret": true}},
    "ko": {"type": "array", "uniqueItems": true,
           "items": {"type": "object", "properties": {"tok": {"type": "string", "writeOnly": true}}}},
    "deep": {"type": "array", "uniqueItems": true, "items": {"type": "object", "properties": {
      "a": {"type": "object", "properties": {"tok": {"type": "string", "x-secret": true}}}}}},
    "ap": {"type": "object", "additionalProperties": {"type": "string", "writeOnly": true},
           "not": {"type": "object"}},
    "eo": {"enum": [{"tok": "a"}], "properties": {"tok": {"writeOnly": true}}},
    "co": {"const": {"tok": "a"}, "properties": {"tok": {"writeOnly": true}}},
    "ty": {"type": "string", "properties": {"tok": {"writeOnly": true}}},
    "nt": {"not": {"type": "object"}, "properties": {"tok": {"writeOnly": true}}},
    "ni": {"type": "array", "items": {"type": "string", "writeOnly": true, "not": {"enum": ["hunter2", "x"]}}},
    "nc": {"type": "string", "writeOnly": true, "not": {"const": "hunter2"}},
    "of": {"oneOf": [
      {"title": "Login", "type": "object", "required": ["user"],
       "properties": {"user": {"type": "string"}, "tok": {"type": "string", "writeOnly": true}}},
      {"title": "Key", "type": "object", "required": ["key"]}
    ]},
    "pi": {"type": "array", "items": {"type": "string", "x-secret": true, "pattern": "^[0-9]+$"}},
    "ct": {"type": "array", "items": {"type": "string", "writeOnly": true}, "contains": {"const": "zzz"}}
  }
}"#;

const DOCUMENT: &str = "ks: [hunter2, hunter2]
kx: [hunter2, hunter2]
ko:
  - tok: hunter2
  - tok: hunter2
deep:
  - a: {tok: hunter2}
  - a: {tok: hunter2}
ap: {k: hunter2}
eo: {tok: hunter2}
co: {tok: hunter2}
ty: {tok: hunter2}
nt: {tok: hunter2}
ni: [hunter2]
nc: hunter2
of: {tok: hunter2}
pi: [hunter2]
ct: [hunter2]
";

/// Every key of the document: each has a problem.
const KEYS: [&str; 14] = [
    "ks", "kx", "ko", "deep", "ap", "eo", "co", "ty", "nt", "ni", "nc", "of", "pi", "ct",
];

#[test]
fn no_message_quotes_a_secret_inside_its_value_in_any_language() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let text = Language::YAML.convert(DOCUMENT, language).unwrap().text;
        let messages: Vec<String> = schema
            .validate(language, &text)
            .into_iter()
            .map(|problem| problem.message)
            .collect();
        for message in &messages {
            if message.contains("hunter2") {
                failures.push(format!("{language}: quotes the secret: {message}"));
            }
        }
        for key in KEYS {
            let at = format!("/{key}");
            if !messages.iter().any(|message| {
                message == &at
                    || message.starts_with(&format!("{at}:"))
                    || message.starts_with(&format!("{at}/"))
            }) {
                failures.push(format!("{language}: no problem at {at}: {messages:#?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_value_holding_a_secret_is_told_with_the_secret_hidden() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let messages: Vec<String> = schema
        .validate(Language::YAML, DOCUMENT)
        .into_iter()
        .map(|problem| problem.message)
        .collect();
    for wanted in [
        "/ks: the list holds the secret value more than once: each item must differ",
        "/ko: the list holds {\"tok\":…} more than once: each item must differ",
        "/deep: the list holds {\"a\":{\"tok\":…}} more than once: each item must differ",
        "/eo: {\"tok\":…} is not one of {\"tok\":\"a\"}",
        "/ty: {\"tok\":…} is not of type \"string\"",
        "/nt: {\"tok\":…} must not be of type \"object\"",
        "/ni/0: the secret value is not allowed here",
        "/nc: the secret value is not allowed here",
    ] {
        assert!(
            messages.iter().any(|message| message == wanted),
            "{wanted:?} not in {messages:#?}"
        );
    }
}
