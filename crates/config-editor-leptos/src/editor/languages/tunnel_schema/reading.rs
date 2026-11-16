//! How documents read: empty documents, XML text typed by the schema, and HCL
//! templates.

use crate::editor::languages::Language;

#[test]
fn a_yaml_file_of_only_comments_is_an_empty_document() {
    for source in ["# just a note\n", "---\n# note\n", "---\n...\n"] {
        assert_eq!(
            Language::YAML.parse(source).unwrap(),
            serde_json::json!({}),
            "{source:?}"
        );
    }
}

#[test]
fn an_empty_xml_element_is_empty_text_where_text_is_expected() {
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    assert_eq!(
        schema.validate(Language::XML, "<public_address></public_address>"),
        []
    );
    let mixed = schema.validate(Language::XML, "<web>stray<hsts>true</hsts></web>");
    assert!(
        mixed
            .iter()
            .any(|problem| problem.message.contains("text is not allowed")),
        "{mixed:?}"
    );
}

#[test]
fn xml_text_is_typed_by_the_schema_and_kept_as_written() {
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    for text in ["123", "true", "1.5"] {
        let document = format!("<public_address>{text}</public_address>");
        assert_eq!(schema.validate(Language::XML, &document), [], "{text}");
    }
    assert_eq!(
        Language::XML.parse("<a>1e3</a>").unwrap(),
        serde_json::json!({"a": "1e3"})
    );
    assert_eq!(
        Language::XML.parse("<a>15</a>").unwrap(),
        serde_json::json!({"a": 15})
    );
}

#[test]
fn hcl_values_written_in_place_escape_templates() {
    let bridge_host = "listen = 7835\npublic_address = \"$${HOST}\"\n";
    let value = Language::HCL.parse(bridge_host).unwrap();
    assert_eq!(value["public_address"], "${HOST}");
    assert_eq!(
        Language::HCL
            .behavior()
            .format_value(&serde_json::json!("${HOST}.example.com")),
        "\"$${HOST}.example.com\""
    );
}
