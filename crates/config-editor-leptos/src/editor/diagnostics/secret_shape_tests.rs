//! P1: a problem never quotes a secret, whatever shape the field holding
//! it reads in: a list where the schema wants one map (an XML element
//! written twice, a list typed in), a string where it wants a map or a
//! list (a broken text read as one scalar), a list at the root, in every
//! language.

use super::super::Language;
use super::validate_with;
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
