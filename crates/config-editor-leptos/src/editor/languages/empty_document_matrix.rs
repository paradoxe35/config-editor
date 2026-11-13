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
