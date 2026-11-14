//! A key added in a view under a prefixed element (QA `E33-3`: a new
//! `<include-filter>` under `<context:component-scan>`, an `<annotation>`
//! under `<xs:element>`, a `Qty` under `<m:GetPrice>` were each saved in
//! the default namespace): the new element takes the prefix of its
//! siblings when it has some, else its parent's. Each new element's
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
            value: json!({"type": "regex", "expression": "x"}),
            wanted: vec![(&["beans", "component-scan", "exclude-filter"], CONTEXT)],
        },
        Case {
            label: "spring component-scan without children",
            host: spring("<context:component-scan base-package=\"a\"/>"),
            parent: "/beans/component-scan",
            key: "include-filter",
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
            value: json!("n"),
            wanted: vec![(&["Envelope", "Body", "Note"], SOAP)],
        },
        Case {
            label: "p:list",
            host: "<p:list xmlns:p=\"urn:p\">\n  <p:v>1</p:v>\n</p:list>\n".to_string(),
            parent: "/list",
            key: "w",
            value: json!(2),
            wanted: vec![(&["list", "w"], "urn:p")],
        },
        Case {
            label: "unprefixed siblings under a prefixed item",
            host: "<list>\n  <a:item xmlns:a=\"urn:one\">\n    <v>1</v>\n  </a:item>\n</list>\n"
                .to_string(),
            parent: "/list/item",
            key: "w",
            value: json!(2),
            wanted: vec![(&["list", "item", "w"], "")],
        },
    ]
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
                    map.insert(case.key.to_string(), case.value.clone());
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
