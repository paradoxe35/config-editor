//! The first key completed in an empty (or blank) document gives a
//! document that reads back with it, in every language: JSON opens its
//! root object around it (`{\n  "port": 8080\n}`, the caret inside),
//! whether the key was asked for on nothing or typed first (`po|`,
//! `"po|"`), and a block's required children are indented inside it.
//!
//! `|` marks the caret.

use super::super::completion::accept_marked;
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;
use serde_json::json;

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{
            "port":{"type":"integer","default":8080},
            "limits":{"type":"object","required":["max"],"properties":{"max":{"type":"integer","default":5}}}
        }}"#,
    )
    .unwrap()
}

/// Blank documents, and the key's start typed into one.
const BLANK: [&str; 9] = [
    "|", "|\n", "\n|", "\n|\n", "  |  ", "po|", "\npo|\n", "\"po|", "\"po|\"",
];

#[test]
fn the_first_json_key_opens_the_root_object() {
    let mut failures = Vec::new();
    for marked in BLANK {
        let accepted = accept_marked(marked, "port", &schema(), Language::JSON.behavior());
        if accepted != "{\n  \"port\": 8080|\n}\n" {
            failures.push(format!("{marked:?}: {accepted:?}"));
        }
    }
    let block = accept_marked("li|", "limits", &schema(), Language::JSON.behavior());
    if block.replace('|', "") != "{\n  \"limits\": {\n    \"max\": 5\n  }\n}\n" {
        failures.push(format!("a block: {block:?}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_first_key_of_a_blank_document_reads_back_in_every_language() {
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        // Quotes start a key in JSON only.
        for marked in BLANK.iter().filter(|marked| !marked.contains('"')) {
            let accepted = accept_marked(marked, "port", &schema(), language.behavior());
            let read = language.parse(&accepted.replace('|', ""));
            if read.as_ref().ok() != Some(&json!({"port": 8080})) {
                failures.push(format!(
                    "{language} {marked:?}: {accepted:?} reads {read:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A schema whose root is a list of objects: the first key of an empty
/// document opens the list's first item, in the languages whose root can
/// be a list.
#[test]
fn the_first_key_under_a_list_root_opens_its_first_item() {
    let schema = build_schema(
        r#"{"type":"array","items":{"type":"object","properties":{
            "id":{"type":"integer","default":7},
            "limits":{"type":"object","required":["max"],"properties":{"max":{"type":"integer","default":5}}}
        }}}"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for language in [Language::YAML, Language::JSON] {
        for marked in BLANK
            .iter()
            .filter(|marked| !marked.contains('"') || language == Language::JSON)
        {
            for (key, wanted) in [
                ("id", json!([{"id": 7}])),
                ("limits", json!([{"limits": {"max": 5}}])),
            ] {
                let marked = marked.replace("po", &key[..1]);
                let accepted = accept_marked(&marked, key, &schema, language.behavior());
                let read = language.parse(&accepted.replace('|', ""));
                if read.as_ref().ok() != Some(&wanted) {
                    failures.push(format!(
                        "{language} {marked:?}: {accepted:?} reads {read:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A schema whose root is a list of values: each value its items allow is
/// offered in an empty document, written as the list's first item.
#[test]
fn the_values_of_a_list_root_are_offered_as_its_first_item() {
    let mut failures = Vec::new();
    for (items, label, wanted) in [
        (r#"{"enum":["a","b"]}"#, "b", json!(["b"])),
        (r#"{"type":"boolean"}"#, "true", json!([true])),
        (r#"{"const":3}"#, "3", json!([3])),
    ] {
        let schema = build_schema(&format!(r#"{{"type":"array","items":{items}}}"#)).unwrap();
        for language in [Language::YAML, Language::JSON] {
            for marked in ["|", "|\n", "\n|\n", "  |  "] {
                let accepted = accept_marked(marked, label, &schema, language.behavior());
                let read = language.parse(&accepted.replace('|', ""));
                if read.as_ref().ok() != Some(&wanted) {
                    failures.push(format!(
                        "{language} {items} {marked:?}: {accepted:?} reads {read:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The caret before a closer on its own line (`{\n|}`): the entry goes on a
/// line of its own inside, indented, the closer after it.
#[test]
fn a_json_key_before_a_closer_on_its_line_is_written_on_its_own_line() {
    let cases = [
        ("{\n|}", "port", "{\n  \"port\": 8080|\n}"),
        (
            "{\n  \"limits\": {\n  |}\n}",
            "max",
            "{\n  \"limits\": {\n    \"max\": 5|\n  }\n}",
        ),
    ];
    let mut failures = Vec::new();
    for (marked, key, wanted) in cases {
        let accepted = accept_marked(marked, key, &schema(), Language::JSON.behavior());
        if accepted != wanted {
            failures.push(format!("{marked:?}: {accepted:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
