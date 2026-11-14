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
            let default = property
                .get("default")
                .or_else(|| property.get("const"))
                .unwrap();
            let kept = language_keeps(language.behavior(), default, property, &bundle.schema);
            let marked = format!("{}|", &key[..1]);
            let accepted = accept_marked(&marked, key, &bundle, language.behavior());
            let text = accepted.replace('|', "");
            let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema))
                .map(|read| read.get(key.as_str()).cloned().unwrap_or(Value::Null));
            // A value the language does not keep is not written (an empty
            // value is left to type), never written as another one.
            let good = match &read {
                Ok(read) if kept => read == default,
                Ok(read) => empty(read) || read == default,
                Err(_) => !kept,
            };
            if !good {
                failures.push(format!("{language} {key}: {text:?} reads {read:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Whether `value` is what an empty value left to type reads as: nothing
/// (an XML `<l></l>` read as a list is one empty item).
fn empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.iter().all(empty),
        Value::Object(members) => members.values().all(empty),
        _ => false,
    }
}
