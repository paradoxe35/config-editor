//! H25-E2: an XML attribute holds a single value. One set to a list, an
//! empty list or a map from another view waits with a problem, as
//! `serialize` refuses it, whether the view spells it `@a` (a child has its
//! name) or `a`: it is never written `a="1 2"`, nor turned into elements.

use super::Language;
use super::save_matrix::{report, save_value};
use serde_json::{Value, json};

type Change = fn(&mut Value);

const REASON: &str = "an XML attribute holds a single value";

#[test]
fn an_attribute_set_to_a_list_or_a_map_waits_from_every_view() {
    let beside_child = "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <z>0</z>\n</r>\n";
    let alone = "<r a=\"1\" b=\"x\">\n  <z>0</z>\n</r>\n";
    let cases: [(&str, &str, Change); 7] = [
        ("`@a` a list", beside_child, |value| {
            value["r"]["@a"] = json!([1, 2]);
        }),
        ("`@a` an empty list", beside_child, |value| {
            value["r"]["@a"] = json!([]);
        }),
        ("`@a` a map", beside_child, |value| {
            value["r"]["@a"] = json!({"k": 1});
        }),
        ("`a` a list", alone, |value| value["r"]["a"] = json!([1, 2])),
        ("`a` an empty list", alone, |value| {
            value["r"]["a"] = json!([])
        }),
        ("`a` a map", alone, |value| {
            value["r"]["a"] = json!({"k": 1})
        }),
        ("`b` a list", alone, |value| {
            value["r"]["b"] = json!(["x", "y"])
        }),
    ];
    let mut failures = Vec::new();
    for (label, host, change) in cases {
        for view in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            match save_value(Language::XML, view, host, &change) {
                Err(error) if error.contains(REASON) => {}
                Err(error) => failures.push(format!("{label} via {view}: {error}")),
                Ok(saved) => failures.push(format!("{label} via {view} saved:\n{saved}")),
            }
        }
    }
    report(failures);
}

/// A child element turned into a list or map stays elements: only an
/// attribute waits.
#[test]
fn a_child_element_set_to_a_list_or_a_map_is_written_as_elements() {
    let host = "<r a=\"1\">\n  <c>2</c>\n</r>\n";
    let mut failures = Vec::new();
    for (change, expected) in [
        (json!([2, 3]), "<r a=\"1\">\n  <c>2</c>\n  <c>3</c>\n</r>\n"),
        (
            json!({"k": 1}),
            "<r a=\"1\">\n  <c>\n    <k>1</k>\n  </c>\n</r>\n",
        ),
    ] {
        for view in [Language::YAML, Language::JSON, Language::TOML] {
            match save_value(Language::XML, view, host, &|value| {
                value["r"]["c"] = change.clone();
            }) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!("{change} via {view}:\n{saved}")),
                Err(error) => failures.push(format!("{change} via {view}: {error}")),
            }
        }
    }
    report(failures);
}

/// `serialize` says the same, and writes a lone `@b` (no child `b`) as the
/// attribute it is, not as a loss at `/r/b`.
#[test]
fn serialize_refuses_an_attribute_list_and_writes_a_lone_at_key() {
    let xml = Language::XML;
    let mut failures = Vec::new();
    for value in [
        json!({"r": {"@a": [1, 2]}}),
        json!({"r": {"@a": []}}),
        json!({"r": {"@a": {"k": 1}}}),
    ] {
        match xml.serialize(&value) {
            Err(error) if error.to_string().contains(REASON) => {}
            other => failures.push(format!("{value}: {other:?}")),
        }
    }
    match xml.serialize(&json!({"r": {"@b": "x"}})) {
        Ok(text) if text == "<r b=\"x\"/>\n" => {}
        other => failures.push(format!("lone @b: {other:?}")),
    }
    report(failures);
}
