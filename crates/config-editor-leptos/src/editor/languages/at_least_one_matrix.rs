//! The "at least one of" idiom (Cargo's `DetailedDependency`): an object's
//! keys declared in `properties`, and an `anyOf` whose options each require
//! one of them, naming it with a `true` schema. Each key is offered once,
//! with its type and description from `properties`, in every language.
//!
//! `|` marks the caret.

use super::super::completion::completions;
use super::super::diagnostics::build_schema;
use super::Language;

const SCHEMA: &str = r#"{"type":"object","properties":{"dep":{"type":"object",
    "properties":{
        "version":{"type":"string","description":"The version wanted."},
        "path":{"type":"string","description":"A local path."},
        "optional":{"type":"boolean"}
    },
    "allOf":[{"anyOf":[
        {"properties":{"version":true},"required":["version"]},
        {"properties":{"path":true},"required":["path"]}
    ]}],
    "additionalProperties":false}}}"#;

#[test]
fn each_key_of_an_at_least_one_of_idiom_is_offered_once_with_its_type() {
    let schema = build_schema(SCHEMA).unwrap();
    let cases = [
        (Language::YAML, "dep:\n  |\n"),
        (Language::JSON, "{\n  \"dep\": {\n    |\n  }\n}\n"),
        (Language::TOML, "[dep]\n|\n"),
        (Language::HCL, "dep {\n  |\n}\n"),
        (Language::XML, "<dep>\n  |\n</dep>\n"),
    ];
    let mut failures = Vec::new();
    for (language, marked) in cases {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let offered = completions(
            &source,
            source[..at].encode_utf16().count(),
            true,
            Some(&schema),
            true,
            language.behavior(),
        );
        let mut shown: Vec<(String, String, String)> = offered
            .iter()
            .map(|option| {
                (
                    option.label.clone(),
                    option.type_label.clone(),
                    option.detail.clone(),
                )
            })
            .collect();
        shown.sort();
        let wanted = [
            ("optional", "boolean", ""),
            ("path", "string", "A local path."),
            ("version", "string", "The version wanted."),
        ]
        .map(|(label, kind, detail)| (label.to_string(), kind.to_string(), detail.to_string()));
        if shown != wanted {
            failures.push(format!("{language}: {shown:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
