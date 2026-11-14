//! A key added in a view under a prefixed element (QA `E33-3`: a new
//! `<include-filter>` under `<context:component-scan>`, an `<annotation>`
//! under `<xs:element>`, a `Qty` under `<m:GetPrice>` were each saved in
//! the default namespace): the new element takes the prefix of its
//! nearest sibling when it has some (the one before it, else after: QA
//! `E34-3`, siblings of two prefixes), else its parent's. Each new element's
//! namespace is read as a namespace-aware reader reads it.

use super::xml_prefix_items::expanded_names;
use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use serde_json::{Value, json};

const VIEWS: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

const BEANS: &str = "http://www.springframework.org/schema/beans";
const CONTEXT: &str = "http://www.springframework.org/schema/context";
const XS: &str = "http://www.w3.org/2001/XMLSchema";
const SOAP: &str = "http://www.w3.org/2003/05/soap-envelope";

/// A host, where a key is added (the parent's pointer, the key and its
/// value), and the namespace of each new element (by its local path).
struct Case {
    label: &'static str,
    host: String,
    parent: &'static str,
    key: &'static str,
    /// Its place among the parent's keys; `None` after them.
    at: Option<usize>,
    value: Value,
    wanted: Vec<(&'static [&'static str], &'static str)>,
}

fn cases() -> Vec<Case> {
    let spring = |inside: &str| {
        format!(
            "<beans xmlns=\"{BEANS}\"\n       xmlns:context=\"{CONTEXT}\">\n  {inside}\n</beans>\n"
        )
    };
    vec![
        Case {
            label: "spring component-scan with a filter",
            host: spring(
                "<context:component-scan base-package=\"a\">\n    <context:include-filter type=\"annotation\" expression=\"A\"/>\n  </context:component-scan>",
            ),
            parent: "/beans/component-scan",
            key: "exclude-filter",
            at: None,
            value: json!({"type": "regex", "expression": "x"}),
            wanted: vec![(&["beans", "component-scan", "exclude-filter"], CONTEXT)],
        },
        Case {
            label: "spring component-scan without children",
            host: spring("<context:component-scan base-package=\"a\"/>"),
            parent: "/beans/component-scan",
            key: "include-filter",
            at: None,
            value: json!({"type": "regex", "expression": "x"}),
            wanted: vec![(&["beans", "component-scan", "include-filter"], CONTEXT)],
        },
        Case {
            label: "xsd element without children",
            host: format!(
                "<xs:schema xmlns:xs=\"{XS}\">\n  <xs:element name=\"a\" type=\"xs:string\"/>\n</xs:schema>\n"
            ),
            parent: "/schema/element",
            key: "annotation",
            at: None,
            value: json!({"documentation": "d"}),
            wanted: vec![
                (&["schema", "element", "annotation"], XS),
                (&["schema", "element", "annotation", "documentation"], XS),
            ],
        },
        Case {
            label: "xsd element with a child",
            host: format!(
                "<xs:schema xmlns:xs=\"{XS}\">\n  <xs:element name=\"a\">\n    <xs:simpleType>\n      <xs:restriction base=\"xs:string\"/>\n    </xs:simpleType>\n  </xs:element>\n</xs:schema>\n"
            ),
            parent: "/schema/element",
            key: "annotation",
            at: None,
            value: json!({"documentation": "d"}),
            wanted: vec![
                (&["schema", "element", "annotation"], XS),
                (&["schema", "element", "annotation", "documentation"], XS),
            ],
        },
        Case {
            label: "soap body item",
            host: format!(
                "<soap:Envelope xmlns:soap=\"{SOAP}\">\n  <soap:Body>\n    <m:GetPrice xmlns:m=\"urn:m\">\n      <m:Item>Apples</m:Item>\n    </m:GetPrice>\n  </soap:Body>\n</soap:Envelope>\n"
            ),
            parent: "/Envelope/Body/GetPrice",
            key: "Qty",
            at: None,
            value: json!(2),
            wanted: vec![(&["Envelope", "Body", "GetPrice", "Qty"], "urn:m")],
        },
        Case {
            label: "soap body beside an element declaring its own prefix",
            host: format!(
                "<soap:Envelope xmlns:soap=\"{SOAP}\">\n  <soap:Body>\n    <m:GetPrice xmlns:m=\"urn:m\">\n      <m:Item>Apples</m:Item>\n    </m:GetPrice>\n  </soap:Body>\n</soap:Envelope>\n"
            ),
            parent: "/Envelope/Body",
            key: "Note",
            at: None,
            value: json!("n"),
            wanted: vec![(&["Envelope", "Body", "Note"], SOAP)],
        },
        Case {
            label: "p:list",
            host: "<p:list xmlns:p=\"urn:p\">\n  <p:v>1</p:v>\n</p:list>\n".to_string(),
            parent: "/list",
            key: "w",
            at: None,
            value: json!(2),
            wanted: vec![(&["list", "w"], "urn:p")],
        },
        Case {
            label: "unprefixed siblings under a prefixed item",
            host: "<list>\n  <a:item xmlns:a=\"urn:one\">\n    <v>1</v>\n  </a:item>\n</list>\n"
                .to_string(),
            parent: "/list/item",
            key: "w",
            at: None,
            value: json!(2),
            wanted: vec![(&["list", "item", "w"], "")],
        },
        mixed("after b:y", None, "urn:b"),
        mixed("between a:x and b:y", Some(1), "urn:a"),
        mixed("before a:x", Some(0), "urn:a"),
        mixed_reversed("between b:y and a:x", Some(1), "urn:b"),
        mixed_reversed("before b:y", Some(0), "urn:b"),
    ]
}

/// Siblings of two prefixes, `a:x` then `b:y`: a key added `at` takes
/// the nearest's.
fn mixed(label: &'static str, at: Option<usize>, namespace: &'static str) -> Case {
    Case {
        label,
        host: "<root xmlns:a=\"urn:a\" xmlns:b=\"urn:b\">\n  <a:cfg>\n    <a:x>1</a:x>\n    <b:y>2</b:y>\n  </a:cfg>\n</root>\n".to_string(),
        parent: "/root/cfg",
        key: "z",
        at,
        value: json!(3),
        wanted: vec![(&["root", "cfg", "z"], namespace)],
    }
}

/// [`mixed`] with `b:y` first.
fn mixed_reversed(label: &'static str, at: Option<usize>, namespace: &'static str) -> Case {
    Case {
        host: "<root xmlns:a=\"urn:a\" xmlns:b=\"urn:b\">\n  <a:cfg>\n    <b:y>2</b:y>\n    <a:x>1</a:x>\n  </a:cfg>\n</root>\n".to_string(),
        ..mixed(label, at, namespace)
    }
}

/// `map` with `key` set to `value` at its place `at` (`None`: last).
fn inserted(map: &mut serde_json::Map<String, Value>, key: &str, at: Option<usize>, value: Value) {
    let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
    entries.insert(at.unwrap_or(entries.len()), (key.to_string(), value));
    map.extend(entries);
}

#[test]
fn a_key_added_under_a_prefixed_element_takes_its_siblings_or_parents_prefix() {
    let mut failures = Vec::new();
    for case in cases() {
        for edit in VIEWS {
            let label = format!("{}, via {edit}", case.label);
            let saved = save_value(Language::XML, edit, &case.host, &|value| {
                if let Some(map) = value
                    .pointer_mut(case.parent)
                    .and_then(Value::as_object_mut)
                {
                    inserted(map, case.key, case.at, case.value.clone());
                }
            });
            let saved = match saved {
                Ok(saved) => saved,
                Err(error) => {
                    failures.push(format!("{label}: not saved: {error}"));
                    continue;
                }
            };
            let names = match expanded_names(&saved) {
                Ok(names) => names,
                Err(error) => {
                    failures.push(format!("{label}: {error}:\n{saved}"));
                    continue;
                }
            };
            for (path, namespace) in &case.wanted {
                let found = names
                    .iter()
                    .find(|(found, _)| found.iter().map(String::as_str).eq(path.iter().copied()));
                if found.map(|(_, found)| found.as_str()) != Some(*namespace) {
                    failures.push(format!(
                        "{label}: {} in {:?}, wanted {namespace:?}:\n{saved}",
                        path.join("/"),
                        found.map(|(_, found)| found)
                    ));
                }
            }
        }
    }
    report(failures);
}
