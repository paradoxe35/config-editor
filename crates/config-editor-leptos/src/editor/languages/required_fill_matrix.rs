//! Accepting a key whose object requires children writes them, in every
//! language alike: a required object's own required children with their
//! defaults (`limits: {cpu: 2, mem: 1G}`, not `limits: {}`), and a required
//! array as an empty array (not a YAML `tags:`, which reads as null).
//!
//! `|` marks the caret.

use super::super::completion::accept_marked;
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::{Value, json};

const SCHEMA: &str = r#"{"type":"object","properties":{"svc":{"type":"object",
    "required":["name","limits","tags","port"],
    "properties":{
        "name":{"type":"string"},
        "limits":{"type":"object","required":["cpu","mem"],"properties":{
            "cpu":{"type":"integer","default":2},
            "mem":{"type":"string","default":"1G"}}},
        "tags":{"type":"array","items":{"type":"string"}},
        "port":{"type":"integer","default":80}
    }}}}"#;

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

#[test]
fn required_children_are_filled_alike_in_every_language() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let accepted = accept_marked("sv|", "svc", &bundle, language.behavior());
        let text = accepted.replace('|', "");
        let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema));
        let svc = read.as_ref().ok().and_then(|read| read.get("svc"));
        let field = |key: &str| svc.and_then(|svc| svc.get(key)).cloned();
        let found = (field("limits"), field("tags"), field("port"));
        // XML has no empty list: an empty element is the closest it writes
        // (one empty item, as a schema of strings reads it).
        let tags = if language == Language::XML {
            json!([""])
        } else {
            Value::Array(Vec::new())
        };
        let wanted = (
            Some(json!({"cpu": 2, "mem": "1G"})),
            Some(tags),
            Some(json!(80)),
        );
        if found != wanted {
            failures.push(format!("{language}\n{accepted}\nreads {read:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
