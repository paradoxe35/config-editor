//! `"xml": {"attribute": true}` in the schema: a new value it marks is
//! written as an attribute (converted from every language, and added in
//! another view of an XML host), what the text already writes keeps its
//! form, and completion offers attributes in a start tag and elements in
//! the content.

use serde_json::{Value, json};

use crate::editor::Language;
use crate::editor::diagnostics::Schema;

const SCHEMA: &str = r#"{
  "properties": {
    "server": {
      "properties": {
        "port": {"type": "integer", "xml": {"attribute": true}},
        "proto": {"enum": ["tcp", "udp"], "xml": {"attribute": true}},
        "name": {"type": "string"},
        "tls": {"properties": {"cert": {"type": "string"}}}
      }
    }
  }
}"#;

fn schema() -> Schema {
    Schema::compile(SCHEMA).unwrap()
}

#[test]
fn a_value_the_schema_marks_an_attribute_converts_to_one_from_every_language() {
    let value = json!({"server": {"port": 80, "name": "edge", "tls": {"cert": "c.pem"}}});
    let expected = "<server port=\"80\">\n  <name>edge</name>\n  <tls>\n    <cert>c.pem</cert>\n  </tls>\n</server>\n";
    let mut failures = Vec::new();
    for from in [Language::YAML, Language::JSON, Language::TOML, Language::HCL] {
        let Ok(text) = from.serialize(&value) else {
            continue;
        };
        match schema().convert(from, &text, Language::XML) {
            Ok(converted) if converted.text == expected && converted.losses.is_empty() => {}
            other => failures.push(format!("{from}: {other:?}")),
        }
    }
    // Only attributes: an empty element (the README's example).
    let only = schema()
        .convert(Language::JSON, r#"{"server": {"port": 80}}"#, Language::XML)
        .unwrap();
    assert_eq!(only.text, "<server port=\"80\"/>\n");
    // Without the schema, every value is an element.
    let plain = Language::JSON
        .convert(&value.to_string(), Language::XML)
        .unwrap();
    assert!(plain.text.contains("<port>80</port>"), "{}", plain.text);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_text_keeps_its_form_and_only_a_new_value_follows_the_schema() {
    use crate::editor::bridge::Bridge;
    use std::sync::Arc;
    let bundle = Arc::new(crate::editor::diagnostics::build_schema(SCHEMA).unwrap());
    // `port` written as an element stays one; `proto`, added in the YAML
    // view, is new: an attribute.
    let host = "<server>\n  <port>80</port>\n  <name>edge</name>\n</server>\n";
    let bridge = Bridge::new(Language::XML, Language::YAML, Some(bundle)).unwrap();
    let shown = bridge.to_edit(host).unwrap().text;
    let edited = shown.replace("  name: edge\n", "  name: edge\n  proto: udp\n");
    assert_ne!(edited, shown, "{shown}");
    let saved = bridge.to_host(&edited, host).unwrap();
    assert_eq!(
        saved,
        "<server proto=\"udp\">\n  <port>80</port>\n  <name>edge</name>\n</server>\n"
    );
}

/// The labels the editor offers at the `|` of `marked`.
fn offered(schema: &Schema, marked: &str) -> Vec<String> {
    let at = marked.find('|').unwrap();
    let source = marked.replace('|', "");
    let cursor = source[..at].encode_utf16().count();
    schema
        .suggestions(Language::XML, &source, cursor)
        .iter()
        .map(|suggestion| suggestion.label().to_string())
        .collect()
}

#[test]
fn completion_offers_attributes_in_a_start_tag_and_elements_in_the_content() {
    let schema = schema();
    let mut attributes = offered(&schema, "<server |></server>");
    attributes.sort();
    assert_eq!(attributes, ["port", "proto"]);
    let mut elements = offered(&schema, "<server>\n  |\n</server>");
    elements.sort();
    assert_eq!(elements, ["name", "tls"]);
    // A schema marking none offers every key in the content, and every
    // scalar key in a start tag, as before.
    let plain: Value = serde_json::from_str(&SCHEMA.replace(r#", "xml": {"attribute": true}"#, "")).unwrap();
    let plain = Schema::compile(&plain.to_string()).unwrap();
    for (marked, expected) in [
        ("<server |></server>", &["name", "port", "proto"][..]),
        ("<server>\n  |\n</server>", &["name", "port", "proto", "tls"][..]),
    ] {
        let mut all = offered(&plain, marked);
        all.sort();
        assert_eq!(all, expected, "{marked}");
    }
}
