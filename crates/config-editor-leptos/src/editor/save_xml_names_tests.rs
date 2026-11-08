//! H23-E2: an XML attribute and a child element of one element with the
//! same name (`<r a="1"><a>2</a></r>`). The model keys the attribute
//! `@a`, so the two never merge into one list, and every view edits each
//! without turning the attribute into an element.

use super::Language;
use super::save_matrix::{from_every_view, report};
use serde_json::{Value, json};

const ONE: &str = "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <z>0</z>\n</r>\n";
const TWO: &str = "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <a>3</a>\n  <z>0</z>\n</r>\n";

#[test]
fn an_attribute_and_an_element_of_one_name_are_two_values() {
    let xml = Language::XML.behavior();
    assert_eq!(
        xml.parse(ONE).unwrap(),
        json!({"r": {"@a": 1, "a": 2, "z": 0}})
    );
    assert_eq!(
        xml.parse(TWO).unwrap(),
        json!({"r": {"@a": 1, "a": [2, 3], "z": 0}})
    );
    // Self-closing children too, and a name only an attribute has stays.
    assert_eq!(
        xml.parse("<r a=\"1\" b=\"2\"><a/></r>").unwrap(),
        json!({"r": {"@a": 1, "b": 2, "a": {}}})
    );
    let span = |text: &str, pointer: &str| {
        let span = xml.value_span(text, pointer)?;
        let line = text.lines().nth(span.line - 1)?;
        Some(line[span.start_column - 1..span.end_column - 1].to_string())
    };
    assert_eq!(span(TWO, "/r/@a").as_deref(), Some("\"1\""));
    assert_eq!(span(TWO, "/r/a/0").as_deref(), Some("2"));
    assert_eq!(span(TWO, "/r/a/1").as_deref(), Some("3"));
    assert_eq!(span(ONE, "/r/a").as_deref(), Some("2"));
    let key = |text: &str, pointer: &str| xml.key_span(text, pointer).map(|span| span.line);
    assert_eq!(key(TWO, "/r/@a"), Some(1));
    assert_eq!(key(TWO, "/r/a/1"), Some(3));
}

#[test]
fn every_view_edits_an_attribute_and_an_element_of_one_name_apart() {
    let mut failures = Vec::new();
    type Case = (&'static str, &'static str, fn(&mut Value), &'static str);
    let cases: [Case; 8] = [
        (
            "attribute edited",
            ONE,
            |value| value["r"]["@a"] = json!(10),
            "<r a=\"10\">\n  <a>2</a> <!-- c -->\n  <z>0</z>\n</r>\n",
        ),
        (
            "element edited",
            ONE,
            |value| value["r"]["a"] = json!(20),
            "<r a=\"1\">\n  <a>20</a> <!-- c -->\n  <z>0</z>\n</r>\n",
        ),
        (
            "another key edited",
            ONE,
            |value| value["r"]["z"] = json!(9),
            "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <z>9</z>\n</r>\n",
        ),
        (
            "element removed",
            TWO,
            |value| value["r"]["a"] = json!(3),
            "<r a=\"1\">\n  <a>3</a>\n  <z>0</z>\n</r>\n",
        ),
        (
            "second element edited",
            TWO,
            |value| value["r"]["a"] = json!([2, 30]),
            "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <a>30</a>\n  <z>0</z>\n</r>\n",
        ),
        (
            "element appended",
            TWO,
            |value| value["r"]["a"] = json!([2, 3, 5]),
            "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <a>3</a>\n  <a>5</a>\n  <z>0</z>\n</r>\n",
        ),
        (
            "elements reordered",
            TWO,
            |value| value["r"]["a"] = json!([3, 2]),
            "<r a=\"1\">\n  <a>3</a>\n  <a>2</a> <!-- c -->\n  <z>0</z>\n</r>\n",
        ),
        (
            "attribute and elements edited",
            TWO,
            |value| {
                value["r"]["@a"] = json!(7);
                value["r"]["a"] = json!([3, 2, 5]);
            },
            "<r a=\"7\">\n  <a>3</a>\n  <a>2</a> <!-- c -->\n  <a>5</a>\n  <z>0</z>\n</r>\n",
        ),
    ];
    for (label, host, change, expected) in cases {
        from_every_view(&mut failures, label, Language::XML, host, &change, expected);
    }
    report(failures);
}

#[test]
fn an_attribute_beside_an_element_of_its_name_converts_both_ways() {
    let json = Language::XML.convert(TWO, Language::JSON).unwrap();
    assert!(json.losses.is_empty(), "{:?}", json.losses);
    let back = Language::JSON.convert(&json.text, Language::XML).unwrap();
    assert!(back.losses.is_empty(), "{:?}", back.losses);
    assert_eq!(
        Language::XML.behavior().parse(&back.text).unwrap(),
        json!({"r": {"@a": 1, "a": [2, 3], "z": 0}})
    );
}
