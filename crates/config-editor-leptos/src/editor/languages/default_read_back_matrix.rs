//! A key's `default` a language keeps ([`language_keeps`]) is written by
//! its suggestion as text that reads back as that default, in every
//! language: strings YAML or XML would read otherwise (`a\r\nb`, `~`,
//! `<tag>&amp;`), and empty lists and objects (a YAML `[]`, not an item
//! left empty).
//!
//! `|` marks the caret.

use super::super::completion::{accept_marked, language_keeps};
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::Value;

const SCHEMA: &str = include_str!("../../../tests/fixtures/defaults.schema.json");

#[test]
fn a_kept_default_reads_back_as_itself() {
    let bundle = build_schema(SCHEMA).unwrap();
    let properties = bundle.schema["properties"].as_object().unwrap().clone();
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        for (key, property) in properties.iter().filter(|(key, _)| *key != "a") {
            let default = &property["default"];
            if !language_keeps(language.behavior(), default, property, &bundle.schema) {
                continue;
            }
            let marked = format!("{}|", &key[..1]);
            let accepted = accept_marked(&marked, key, &bundle, language.behavior());
            let text = accepted.replace('|', "");
            let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema))
                .map(|read| read.get(key.as_str()).cloned().unwrap_or(Value::Null));
            if read.as_ref().ok() != Some(default) {
                failures.push(format!("{language} {key}: {text:?} reads {read:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
