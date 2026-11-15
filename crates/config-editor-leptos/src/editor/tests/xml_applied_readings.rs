//! XML text read by every subschema that applies at its path, not by the
//! first `properties` entry: a field the base `properties` only describes
//! (`workers: {description}`) is typed by the `allOf` (through `$ref`),
//! the `then` its object takes, a dependent schema, a pattern, an
//! `additionalProperties` or the option the object is in. Native XML
//! validates and converts it as typed, and a value typed in the XML view of
//! another host is saved as the view read it.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::diagnostics::{Schema, build_schema};

/// `server` schemas typing `workers` as "an integer ≥ 1 or `auto`" beside
/// a base property that only describes it.
fn shapes() -> Vec<(&'static str, Value)> {
    let workers = json!({"oneOf": [{"type": "integer", "minimum": 1}, {"const": "auto"}]});
    let base = json!({"mode": {"type": "string"}, "workers": {"description": "Worker count"}});
    let with = |extra: Value| {
        let mut server = json!({"type": "object", "properties": base.clone()});
        for (keyword, schema) in extra.as_object().unwrap() {
            server[keyword] = schema.clone();
        }
        server
    };
    vec![
        (
            "plain",
            json!({"type": "object", "properties": {"mode": {}, "workers": workers}}),
        ),
        (
            "allOf",
            with(json!({"allOf": [{"properties": {"workers": workers}}]})),
        ),
        (
            "allOf $ref",
            with(json!({"allOf": [{"$ref": "#/$defs/w"}]})),
        ),
        (
            "if/then",
            with(json!({
                "if": {"properties": {"mode": {"const": "fast"}}},
                "then": {"properties": {"workers": workers}},
                "else": {"properties": {"workers": {"type": "string"}}}
            })),
        ),
        (
            "dependentSchemas",
            with(json!({"dependentSchemas": {"mode": {"properties": {"workers": workers}}}})),
        ),
        (
            "patternProperties",
            with(json!({"patternProperties": {"^work": workers}})),
        ),
        (
            "additionalProperties",
            with(json!({"allOf": [{"properties": {"mode": {}}, "additionalProperties": workers}]})),
        ),
        (
            "oneOf",
            with(json!({"oneOf": [
                {"properties": {"mode": {"const": "fast"}, "workers": workers}},
                {"properties": {"mode": {"const": "slow"}, "workers": {"type": "string"}}}
            ]})),
        ),
    ]
}

fn root(server: Value) -> String {
    json!({
        "type": "object",
        "properties": {"server": server},
        "$defs": {"w": {"properties": {"workers":
            {"oneOf": [{"type": "integer", "minimum": 1}, {"const": "auto"}]}}}}
    })
    .to_string()
}

fn messages(schema: &Schema, language: Language, text: &str) -> Vec<String> {
    schema
        .validate(language, text)
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

#[test]
fn a_value_typed_in_an_xml_view_is_saved_as_every_applying_subschema_reads_it() {
    let mut failures = Vec::new();
    for (name, server) in shapes() {
        let text = root(server);
        let bundle = Arc::new(build_schema(&text).unwrap());
        let schema = Schema::compile(&text).unwrap();
        let document = json!({"server": {"mode": "fast", "workers": "auto"}});
        for host_language in [
            Language::JSON,
            Language::YAML,
            Language::TOML,
            Language::HCL,
        ] {
            let host = host_language.serialize(&document).unwrap();
            let bridge = Bridge::new(host_language, Language::XML, Some(bundle.clone())).unwrap();
            let shown = bridge.to_edit(&host).unwrap().text;
            let edited = shown.replacen(">auto<", ">8<", 1);
            let in_view = messages(&schema, Language::XML, &edited);
            if !in_view.is_empty() {
                failures.push(format!("{name}: view of {host_language}: {in_view:?}"));
            }
            match bridge.to_host(&edited, &host) {
                Ok(saved) => {
                    let read = host_language.parse(&saved);
                    let found = messages(&schema, host_language, &saved);
                    if read.ok() != Some(json!({"server": {"mode": "fast", "workers": 8}}))
                        || !found.is_empty()
                    {
                        failures.push(format!(
                            "{name}: saved to {host_language}: {saved:?} {found:?}"
                        ));
                    }
                }
                Err(error) => failures.push(format!("{name}: {host_language}: {error:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn native_xml_reads_a_field_by_every_applying_subschema() {
    let mut failures = Vec::new();
    for (name, server) in shapes() {
        let schema = Schema::compile(&root(server)).unwrap();
        for (xml, workers) in [
            (
                "<server><mode>fast</mode><workers>8</workers></server>\n",
                json!(8),
            ),
            (
                "<server><mode>fast</mode><workers>auto</workers></server>\n",
                json!("auto"),
            ),
            ("<server mode=\"fast\" workers=\"8\"/>\n", json!(8)),
        ] {
            let found = messages(&schema, Language::XML, xml);
            if !found.is_empty() {
                failures.push(format!("{name}: {xml:?}: {found:?}"));
            }
            let converted = schema.convert(Language::XML, xml, Language::JSON).unwrap();
            let read: Value = serde_json::from_str(&converted.text).unwrap();
            if read.pointer("/server/workers") != Some(&workers) {
                failures.push(format!("{name}: {xml:?} converts to {}", converted.text));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// List items whose `port` and `flag` an `if`/`then`/`else` types beside
/// a base property that allows anything (`{}`) or only describes it.
fn item_shapes() -> Vec<(&'static str, Value)> {
    let item = |base: Value| {
        json!({
            "type": "object",
            "properties": {"kind": {"enum": ["tcp", "unix"]}, "port": base.clone(), "flag": base},
            "if": {"properties": {"kind": {"const": "tcp"}}},
            "then": {"properties": {"port": {"type": "integer"}, "flag": {"type": "boolean"}}},
            "else": {"properties": {"port": {"type": "string"}, "flag": {"type": "string"}}}
        })
    };
    vec![
        ("bare", item(json!({}))),
        ("described", item(json!({"description": "Port or socket"}))),
        (
            "allOf",
            json!({"type": "object", "properties": {"port": {}, "flag": {}},
                "allOf": [{"properties": {"port": {"type": "string"}, "flag": {"type": "string"}}}]}),
        ),
    ]
}

fn items_root(item: Value) -> String {
    json!({"type": "object", "properties": {"svc": {"type": "array", "items": item}}}).to_string()
}

#[test]
fn native_xml_reads_list_items_by_their_conditional_branch() {
    let mut failures = Vec::new();
    for (name, item) in item_shapes() {
        let schema = Schema::compile(&items_root(item)).unwrap();
        let tcp = name != "allOf";
        for (xml, port, flag) in [
            (
                "<svc><kind>unix</kind><port>8080</port><flag>true</flag></svc>\n",
                json!("8080"),
                json!("true"),
            ),
            (
                "<svc kind=\"unix\" port=\"8080\" flag=\"true\"/>\n",
                json!("8080"),
                json!("true"),
            ),
            (
                "<svc><kind>tcp</kind><port>8080</port><flag>true</flag></svc>\n",
                if tcp { json!(8080) } else { json!("8080") },
                if tcp { json!(true) } else { json!("true") },
            ),
        ] {
            let found = messages(&schema, Language::XML, xml);
            if !found.is_empty() {
                failures.push(format!("{name}: {xml:?}: {found:?}"));
            }
            let converted = schema.convert(Language::XML, xml, Language::JSON).unwrap();
            let read: Value = serde_json::from_str(&converted.text).unwrap();
            let item = read.pointer("/svc/0");
            if item.and_then(|item| item.get("port")) != Some(&port)
                || item.and_then(|item| item.get("flag")) != Some(&flag)
            {
                failures.push(format!("{name}: {xml:?} converts to {}", converted.text));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn list_items_typed_in_an_xml_view_are_saved_as_their_branch_reads_them() {
    let (_, item) = item_shapes().remove(0);
    let text = items_root(item);
    let bundle = Arc::new(build_schema(&text).unwrap());
    let mut failures = Vec::new();
    let document = json!({"svc": [
        {"kind": "unix", "port": "/run/a", "flag": "yes"},
        {"kind": "tcp", "port": 1, "flag": true}
    ]});
    for host_language in [Language::JSON, Language::YAML, Language::TOML] {
        let host = host_language.serialize(&document).unwrap();
        let bridge = Bridge::new(host_language, Language::XML, Some(bundle.clone())).unwrap();
        let shown = bridge.to_edit(&host).unwrap().text;
        // The unix item made a tcp one, its port and flag typed anew.
        let edited = shown
            .replacen(">unix<", ">tcp<", 1)
            .replacen(">/run/a<", ">9<", 1)
            .replacen(">yes<", ">false<", 1);
        let saved = bridge
            .to_host(&edited, &host)
            .map(|saved| host_language.parse(&saved));
        let expected = json!({"svc": [
            {"kind": "tcp", "port": 9, "flag": false},
            {"kind": "tcp", "port": 1, "flag": true}
        ]});
        match saved {
            Ok(Ok(read)) if read == expected => {}
            other => failures.push(format!("saved to {host_language}: {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Reading each value by every subschema at its path walks its parent's
/// schemas once per value: a larger object costs the same for each key.
#[test]
fn reading_by_every_applying_subschema_costs_the_same_for_each_key() {
    use crate::editor::completion::conform_arrays;
    use crate::editor::language::Repetition;
    use crate::editor::work_count::{Work, counted};
    let mut failures = Vec::new();
    for (name, server) in shapes() {
        let bundle = build_schema(&root(server)).unwrap();
        let walks = |others: usize| {
            let fields: String = (0..others)
                .map(|index| format!("<k{index}>{index}</k{index}>"))
                .collect();
            let xml = format!("<server><mode>fast</mode><workers>8</workers>{fields}</server>\n");
            let mut value = Language::XML.parse(&xml).unwrap();
            counted(Work::BranchWalk, || {
                conform_arrays(&mut value, &bundle.schema, Repetition::Elements);
            })
            .1
        };
        let each = walks(51) - walks(50);
        let (small, large) = (walks(50), walks(200));
        if large - small != 150 * each {
            failures.push(format!("{name}: {small} then {large}, {each} per key"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
