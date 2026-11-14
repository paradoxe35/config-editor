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

/// The hover card of a key says what its suggestion says: the type a
/// branch declares (not "property" for an option only naming it), and
/// "required" only for a key the object itself requires, not one a
/// single `anyOf`/`oneOf` option does.
#[test]
fn a_key_s_hover_card_agrees_with_its_suggestion() {
    let schema = r#"{"type":"object","properties":{"dep":{"type":"object",
        "required":["name"],
        "properties":{
            "name":{"type":"string"},
            "version":{"type":"string"},
            "git":{"type":"string"}
        },
        "anyOf":[
            {"properties":{"version":true},"required":["version"]},
            {"properties":{"git":true},"required":["git"]}
        ],
        "oneOf":[{"required":["version"]},{"required":["git"]}]}}}"#;
    let compiled = crate::editor::diagnostics::Schema::compile(schema).unwrap();
    let bundle = build_schema(schema).unwrap();
    // Each text, and the lines of `name` and `version` in it.
    let cases = [
        (Language::YAML, "dep:\n  name: n\n  version: \"1\"\n", 3),
        (
            Language::JSON,
            "{\n  \"dep\": {\n    \"name\": \"n\",\n    \"version\": \"1\"\n  }\n}\n",
            6,
        ),
        (Language::TOML, "[dep]\nname = \"n\"\nversion = \"1\"\n", 1),
        (
            Language::HCL,
            "dep {\n  name = \"n\"\n  version = \"1\"\n}\n",
            3,
        ),
        (
            Language::XML,
            "<dep>\n  <name>n</name>\n  <version>1</version>\n</dep>\n",
            4,
        ),
    ];
    let mut failures = Vec::new();
    for (language, text, column) in cases {
        let first = text.lines().position(|line| line.contains("name")).unwrap() + 1;
        for (key, line, required) in [("name", first, true), ("version", first + 1, false)] {
            let hover = compiled.hover(
                language,
                text,
                crate::editor::TextPosition::new(line, column),
            );
            let shown = hover
                .as_ref()
                .map(|hover| (hover.type_label.as_str(), hover.required));
            if shown != Some(("string", required)) {
                failures.push(format!("{language} {key}: {hover:?}"));
            }
        }
        // The suggestion of the key not written says the same.
        let at = text.find("version").unwrap();
        let line_start = text[..at].rfind('\n').unwrap() + 1;
        let source = format!(
            "{}{}",
            &text[..line_start],
            text[line_start..].replacen("version", "vers", 1)
        );
        let caret = source[..line_start].len() + text[line_start..at].len() + 4;
        let offered = completions(
            &source,
            source[..caret].encode_utf16().count(),
            true,
            Some(&bundle),
            true,
            language.behavior(),
        );
        let suggested = offered.iter().find(|option| option.label == "version");
        if suggested.map(|option| option.type_label.as_str()) != Some("string") {
            failures.push(format!(
                "{language} version suggested as {:?}",
                suggested.map(|option| &option.type_label)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
