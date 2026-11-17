//! The "at least one of" idiom (Cargo's `DetailedDependency`): an object's
//! keys declared in `properties`, and an `anyOf` whose options each require
//! one of them, naming it with a `true` schema. Each key is offered once,
//! with its type and description from `properties`, in every language.
//!
//! `|` marks the caret.

use super::super::completion::completions;
use super::super::schema::build_schema;
use super::harness::{At, completing, hovering, key_place, name_range, report, text, write};
use serde_json::json;

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
    let mut failures = Vec::new();
    for language in completing() {
        let at = At::NewLine { object: "/dep" };
        let (source, at) = match write(language, &json!({"dep": {}}), &[], at) {
            Ok(written) => (written.text, written.byte),
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
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
    report(&failures);
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
    let compiled = crate::editor::schema::Schema::compile(schema).unwrap();
    let bundle = build_schema(schema).unwrap();
    let mut failures = Vec::new();
    let document = json!({"dep": {"name": "n", "version": "1"}});
    for language in completing()
        .into_iter()
        .filter(|language| hovering().contains(language))
    {
        let text = match text(language, &document, &[]) {
            Ok(text) => text,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let text = text.as_str();
        for (key, required) in [("name", true), ("version", false)] {
            let Ok((line, column)) = key_place(language, text, &format!("/dep/{key}"), 0) else {
                failures.push(format!("{language} {key}: not placed in {text:?}"));
                continue;
            };
            let hover = compiled.hover(
                language,
                text,
                crate::editor::TextPosition::new(line + 1, column + 1),
            );
            let shown = hover
                .as_ref()
                .map(|hover| (hover.type_label.as_str(), hover.required));
            if shown != Some(("string", required)) {
                failures.push(format!("{language} {key}: {hover:?}"));
            }
        }
        // The suggestion of the key not written says the same: `version`
        // cut to `vers`, the caret after it.
        let Ok((start, end)) = name_range(language, text, "/dep/version") else {
            continue;
        };
        let source = format!("{}vers{}", &text[..start], &text[end..]);
        let caret = start + 4;
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
    report(&failures);
}
