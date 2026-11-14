//! A key more than one `oneOf`/`anyOf` option names is documented, on
//! hover, by the option the document matches, as its suggestion is: in
//! an object and in a list's items, each option matched, the options
//! inline or through `$ref`, in every language.

use super::super::completion::key_hover;
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::{Value, json};

fn option(kind: &str, own: &str) -> Value {
    json!({
        "type": "object", "required": ["kind"], "additionalProperties": false,
        "properties": {
            "kind": {"const": kind},
            own: {"type": "string"},
            "shared": {"type": "string", "description": format!("{kind} shared")}
        }
    })
}

fn schemas() -> Vec<(&'static str, Value)> {
    let options = || vec![option("file", "path"), option("http", "url")];
    let refs = json!([{"$ref": "#/$defs/file"}, {"$ref": "#/$defs/http"}]);
    let defs = json!({"file": option("file", "path"), "http": option("http", "url")});
    let mut schemas = Vec::new();
    for combinator in ["oneOf", "anyOf"] {
        schemas.push((
            combinator,
            json!({"type": "object", "properties": {
                "out": {combinator: options()},
                "outs": {"type": "array", "items": {combinator: options()}}
            }}),
        ));
        schemas.push((
            combinator,
            json!({"type": "object", "$defs": defs, "properties": {
                "out": {combinator: refs},
                "outs": {"type": "array", "items": {combinator: refs}}
            }}),
        ));
    }
    schemas
}

#[test]
fn a_shared_key_is_documented_by_the_matched_option() {
    let mut failures = Vec::new();
    for (name, schema) in schemas() {
        let bundle = build_schema(&schema.to_string()).unwrap();
        for kind in ["file", "http"] {
            let entry = json!({"kind": kind, "shared": "x"});
            for (field, value) in [
                ("out", json!({"out": entry})),
                ("outs", json!({"outs": [entry, entry]})),
            ] {
                for language in [
                    Language::YAML,
                    Language::JSON,
                    Language::TOML,
                    Language::HCL,
                    Language::XML,
                ] {
                    let text = language.serialize(&value).unwrap();
                    let Some(line) = text.lines().position(|line| line.contains("shared")) else {
                        failures.push(format!("{name} {field} {language}: {text:?}"));
                        continue;
                    };
                    let column = text.lines().nth(line).unwrap().find("shared").unwrap() + 2;
                    let hover = key_hover(&text, line, column, Some(&bundle), language.behavior());
                    let said = hover.and_then(|hover| hover.description);
                    let wanted = format!("{kind} shared");
                    if said.as_deref() != Some(wanted.as_str()) {
                        failures.push(format!(
                            "{name} {field} {kind} {language}: said {said:?} in {text:?}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
