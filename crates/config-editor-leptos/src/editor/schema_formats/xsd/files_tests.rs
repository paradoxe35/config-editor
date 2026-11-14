//! Several files: `xs:include`, `xs:import` and `xs:redefine` read from the
//! host's resources (nothing fetched), resolved against the schema's
//! location, a missing one naming itself; their losses say which file.

use serde_json::json;

use super::super::{LossEffect, SchemaFormat, SchemaInput, SchemaResources};
use crate::editor::diagnostics::Schema;

const XS: &str = "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"";

fn schema(body: &str) -> String {
    format!("<xs:schema {XS}>\n{body}\n</xs:schema>\n")
}

fn compile(
    text: &str,
    location: Option<&str>,
    resources: &SchemaResources,
) -> Result<Schema, String> {
    let mut input = SchemaInput::new(text).resources(resources);
    if let Some(location) = location {
        input = input.at(location);
    }
    Schema::compile_as(SchemaFormat::XSD, &input).map_err(|error| error.0)
}

#[test]
fn an_include_brings_its_definitions_resolved_against_the_schema_location() {
    let main = schema(
        r#"<xs:include schemaLocation="types/common.xsd"/><xs:element name="server" type="serverType"/>"#,
    );
    let common = schema(
        r#"<xs:complexType name="serverType"><xs:attribute name="port" type="port"/></xs:complexType>
<xs:simpleType name="port"><xs:restriction base="xs:int"><xs:maxInclusive value="65535"/></xs:restriction></xs:simpleType>"#,
    );
    for (location, key) in [
        (
            Some("https://example.com/schemas/main.xsd"),
            "https://example.com/schemas/types/common.xsd",
        ),
        (Some("schemas/main.xsd"), "schemas/types/common.xsd"),
        (None, "types/common.xsd"),
        // As written, when the resolved location is not among them.
        (
            Some("https://example.com/schemas/main.xsd"),
            "types/common.xsd",
        ),
    ] {
        let resources = SchemaResources::new().with(key, common.clone());
        let compiled =
            compile(&main, location, &resources).unwrap_or_else(|error| panic!("{key}: {error}"));
        assert_eq!(
            compiled.json()["$defs"]["type.serverType"]["properties"]["port"]["maximum"],
            json!(65535),
            "{key}"
        );
    }
    let error = compile(
        &main,
        Some("https://example.com/schemas/main.xsd"),
        &SchemaResources::new(),
    )
    .unwrap_err();
    assert!(
        error.contains("\"types/common.xsd\" is not among the schema's resources: pass its text under \"https://example.com/schemas/types/common.xsd\""),
        "{error}"
    );
}

#[test]
fn an_import_by_location_or_by_namespace_and_includes_of_includes() {
    let main = schema(
        r#"<xs:import namespace="urn:a" schemaLocation="a.xsd"/><xs:import namespace="urn:b"/>
<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="a" type="aType"/><xs:element name="b" type="bType"/></xs:sequence></xs:complexType></xs:element>"#,
    );
    let a = format!(
        "<xs:schema {XS} targetNamespace=\"urn:a\"><xs:include schemaLocation=\"deeper.xsd\"/></xs:schema>"
    );
    let deeper = format!(
        "<xs:schema {XS} targetNamespace=\"urn:a\"><xs:simpleType name=\"aType\"><xs:restriction base=\"xs:int\"/></xs:simpleType></xs:schema>"
    );
    let b = format!(
        "<xs:schema {XS} targetNamespace=\"urn:b\"><xs:simpleType name=\"bType\"><xs:restriction base=\"xs:boolean\"/></xs:simpleType></xs:schema>"
    );
    let resources = SchemaResources::new()
        .with("a.xsd", a)
        .with("deeper.xsd", deeper)
        .with("urn:b", b);
    let compiled = compile(&main, None, &resources).unwrap();
    let element = &compiled.json()["$defs"]["element.r"]["properties"];
    assert_eq!(
        compiled.json()["$defs"]["type.aType"]["type"],
        json!("integer")
    );
    assert_eq!(
        compiled.json()["$defs"]["type.bType"]["type"],
        json!("boolean")
    );
    assert!(element.get("a").is_some() && element.get("b").is_some());
    assert_eq!(SchemaFormat::XSD.imports(&main), ["a.xsd"]);
    // The XML namespace's own schema is known without a resource.
    let xml = schema(
        r#"<xs:import namespace="http://www.w3.org/XML/1998/namespace"/><xs:element name="t"><xs:complexType><xs:attribute ref="xml:lang"/></xs:complexType></xs:element>"#,
    );
    assert!(compile(&xml, None, &SchemaResources::new()).is_ok());
}

#[test]
fn a_redefine_replaces_the_definitions_and_extends_the_originals() {
    let main = schema(
        r#"<xs:redefine schemaLocation="base.xsd">
<xs:complexType name="item"><xs:complexContent><xs:extension base="item"><xs:sequence><xs:element name="extra" type="xs:int"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType>
<xs:simpleType name="code"><xs:restriction base="code"><xs:maxLength value="3"/></xs:restriction></xs:simpleType>
</xs:redefine>
<xs:element name="item" type="item"/>"#,
    );
    let base = schema(
        r#"<xs:complexType name="item"><xs:sequence><xs:element name="code" type="code"/></xs:sequence></xs:complexType>
<xs:simpleType name="code"><xs:restriction base="xs:string"><xs:minLength value="1"/></xs:restriction></xs:simpleType>"#,
    );
    let resources = SchemaResources::new().with("base.xsd", base);
    let compiled = compile(&main, None, &resources).unwrap();
    let defs = &compiled.json()["$defs"];
    let item = &defs["type.item"]["properties"];
    assert!(
        item.get("code").is_some() && item.get("extra").is_some(),
        "{item}"
    );
    assert_eq!(defs["type.code"]["minLength"], json!(1));
    assert_eq!(defs["type.code"]["maxLength"], json!(3));
    assert!(
        compiled
            .losses()
            .iter()
            .all(|loss| loss.construct != "xs:redefine"),
        "{:?}",
        compiled.losses()
    );
}

#[test]
fn a_loss_in_another_file_says_which() {
    let main = schema(r#"<xs:include schemaLocation="keys.xsd"/>"#);
    let keys = schema(
        r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence></xs:complexType>
<xs:key name="k"><xs:selector xpath="a[1]"/><xs:field xpath="."/></xs:key></xs:element>"#,
    );
    let resources = SchemaResources::new().with("keys.xsd", keys);
    let compiled = compile(&main, None, &resources).unwrap();
    let loss = compiled
        .losses()
        .iter()
        .find(|loss| loss.construct == "xs:key")
        .expect("the key's loss");
    assert_eq!(loss.location.as_deref(), Some("keys.xsd"));
    assert_eq!((loss.line, loss.effect), (Some(3), LossEffect::NotChecked));
    assert_eq!(
        loss.to_string(),
        "keys.xsd:3:1: xs:key \"k\" is not checked: its XPath is beyond the subset read (child steps, *, .//, |, a field's final @attribute)"
    );
}

#[test]
fn names_two_namespaces_share_are_a_loss() {
    let main = schema(
        r#"<xs:import namespace="urn:o" schemaLocation="o.xsd"/><xs:simpleType name="t"><xs:restriction base="xs:int"/></xs:simpleType><xs:element name="e" type="t"/>"#,
    );
    let other = format!(
        "<xs:schema {XS} targetNamespace=\"urn:o\"><xs:simpleType name=\"t\"><xs:restriction base=\"xs:string\"/></xs:simpleType></xs:schema>"
    );
    let resources = SchemaResources::new().with("o.xsd", other);
    let compiled = compile(&main, None, &resources).unwrap();
    assert!(
        compiled
            .losses()
            .iter()
            .any(|loss| loss.construct == "namespace clash"
                && loss.effect == LossEffect::Approximated),
        "{:?}",
        compiled.losses()
    );
    assert_eq!(
        compiled.json()["$defs"]["type.t"]["type"],
        json!("integer"),
        "the schema's own first"
    );
}
