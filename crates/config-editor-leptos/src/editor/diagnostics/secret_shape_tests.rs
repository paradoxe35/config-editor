//! P1: a problem never quotes a secret, whatever shape the field holding
//! it reads in: a list where the schema wants one map (an XML element
//! written twice, a list typed in), a string where it wants a map or a
//! list (a broken text read as one scalar), a list at the root, in every
//! language.

use super::super::Language;
use super::{save_message, validate_with};
use crate::editor::schema::Schema;
use crate::strings::default_strings;

const SECRET: &str = "s3cr3tZ";

/// The fields at the root, or inside `config` (an XML document's root).
const SCHEMA: &str = r##"{"type": "object", "properties": {
    "config": {"$ref": "#"},
    "db": {"type": "object", "properties": {
        "user": {"type": "string"},
        "password": {"type": "string", "x-secret": true}
    }},
    "keys": {"type": "array", "items": {"type": "string", "writeOnly": true}}
}}"##;

/// Documents whose secret is read where the schema does not place it.
fn documents() -> Vec<(Language, String)> {
    let mut documents = vec![
        (
            Language::XML,
            format!(
                "<config>\n  <db><user>app</user><password>{SECRET}</password></db>\n  \
                 <db>x</db>\n</config>\n"
            ),
        ),
        (
            Language::XML,
            format!(
                "<config>\n  <db user=\"app\" password=\"{SECRET}\"/>\n  <db user=\"b\"/>\n</config>\n"
            ),
        ),
    ];
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
    ] {
        let values = [
            serde_json::json!({"db": [{"user": "app", "password": SECRET}, 2]}),
            serde_json::json!({"db": format!("user app password {SECRET}")}),
            serde_json::json!({"keys": format!("- {SECRET} - other")}),
        ];
        for value in values {
            if let Ok(text) = language.serialize(&value) {
                documents.push((language, text));
            }
        }
    }
    for language in [Language::YAML, Language::JSON] {
        let root = serde_json::json!([SECRET, "other"]);
        documents.push((language, language.serialize(&root).expect("written")));
    }
    documents
}

#[test]
fn a_problem_never_quotes_a_secret_read_in_another_shape() {
    let schema = Schema::compile(SCHEMA).expect("compiles");
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let problems = validate_with(
            &text,
            Some(&schema.bundle),
            None,
            true,
            language.behavior(),
            &[],
            default_strings(),
        );
        if problems.is_empty() {
            failures.push(format!("{language}: no problem in {text:?}"));
        }
        for problem in problems {
            if problem.message.contains(SECRET) {
                failures.push(format!("{language}: {text:?}: {}", problem.message));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A save's reason quoting a value hides a secret's text however the
/// reason quotes it (as written, as a JSON string, inside a list), in
/// every shape its field reads in, in every language; the words around
/// it stay.
#[test]
fn a_save_reason_never_quotes_a_secret_read_in_any_shape() {
    let schema = Schema::compile(SCHEMA).expect("compiles");
    let mut failures = Vec::new();
    let mut documents = documents();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
    ] {
        let value = serde_json::json!({"db": {"user": "app", "password": SECRET}});
        documents.push((language, language.serialize(&value).expect("written")));
    }
    for (language, text) in documents {
        let data = language.parse(&text).expect("parses");
        let mut quoted = Vec::new();
        quote_strings(&data, &mut quoted);
        for value in quoted.iter().filter(|value| value.contains(SECRET)) {
            let json = serde_json::Value::String(value.clone());
            let message =
                format!("the host reads the text {value} back as [{json}], not {json} (at /db)");
            let shown = save_message(&message, &text, Some(&schema.bundle), language.behavior());
            if shown.contains(SECRET) || !shown.ends_with("(at /db)") {
                failures.push(format!("{language}: {text:?}: {shown}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The strings of `value`.
fn quote_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| quote_strings(item, out)),
        serde_json::Value::Object(map) => map.values().for_each(|child| quote_strings(child, out)),
        _ => {}
    }
}
