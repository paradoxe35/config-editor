//! P2: a number the user edits in an XML view (which writes every value as
//! text) keeps the host's number type where the schema takes both a number
//! and a string there (`listen: 7835` edited to `17835` saves `17835`), and
//! is saved as the string the view read where the schema takes only that:
//! the demo's `listen` takes an integer up to 65535 or any string, so
//! `71835` saves `"71835"`, which the view validated, not a number the host
//! would show a problem for.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::schema::{Schema, build_schema};

/// Schemas for `listen` (or none) and whether each takes `71835` as a
/// number.
fn shapes() -> Vec<(&'static str, Option<Value>, bool)> {
    let with = |listen: Value| {
        Some(json!({
            "type": "object",
            "properties": {"listen": listen, "name": {"type": "string"}},
            "$defs": {
                "Bind": {"anyOf": [
                    {"type": "integer", "minimum": 1, "maximum": 65535},
                    {"type": "string", "examples": ["0.0.0.0:7835", "7835"]}
                ]},
                "Unbounded": {"anyOf": [{"type": "integer"}, {"type": "string"}]}
            }
        }))
    };
    let or_null = |bind: &str| json!({"anyOf": [{"$ref": bind}, {"type": "null"}]});
    vec![
        ("no schema", None, true),
        ("demo page", with(or_null("#/$defs/Bind")), false),
        (
            "demo page unbounded",
            with(or_null("#/$defs/Unbounded")),
            true,
        ),
        (
            "type list",
            with(json!({"type": ["string", "number"]})),
            true,
        ),
        (
            "anyOf",
            with(json!({"anyOf": [{"type": "string"}, {"type": "integer"}]})),
            true,
        ),
        (
            "oneOf",
            with(json!({"oneOf": [{"type": "string"}, {"type": "integer"}]})),
            true,
        ),
        ("no type", with(json!({"description": "anything"})), true),
    ]
}

#[test]
fn a_number_edited_in_an_xml_view_keeps_the_hosts_type_unless_the_schema_takes_only_text() {
    let document = json!({"listen": 7835, "name": "tunnel"});
    let mut failures = Vec::new();
    for (name, schema, takes_big) in shapes() {
        let text = schema.map(|schema| schema.to_string());
        let bundle = text
            .as_deref()
            .map(|text| Arc::new(build_schema(text).unwrap()));
        let compiled = text.as_deref().map(|text| Schema::compile(text).unwrap());
        for host_language in [
            Language::JSON,
            Language::YAML,
            Language::TOML,
            Language::HCL,
        ] {
            let host = host_language.serialize(&document).unwrap();
            let bridge = Bridge::new(host_language, Language::XML, bundle.clone()).unwrap();
            let shown = bridge.to_edit(&host).unwrap().text;
            for (typed, number) in [("17835", true), ("71835", takes_big)] {
                let edited = shown.replacen(">7835<", &format!(">{typed}<"), 1);
                assert_ne!(edited, shown, "{name}: {shown}");
                let listen = if number {
                    json!(typed.parse::<u64>().unwrap())
                } else {
                    json!(typed)
                };
                match bridge.to_host(&edited, &host) {
                    Ok(saved) => {
                        let read = host_language.parse(&saved).ok();
                        let problems = compiled
                            .as_ref()
                            .map(|schema| schema.validate(host_language, &saved))
                            .unwrap_or_default();
                        if read != Some(json!({"listen": listen, "name": "tunnel"}))
                            || !problems.is_empty()
                        {
                            failures.push(format!(
                                "{name}: {typed} in {host_language}: {saved:?} {problems:?}"
                            ));
                        }
                    }
                    Err(error) => failures.push(format!("{name}: {host_language}: {error:?}")),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
