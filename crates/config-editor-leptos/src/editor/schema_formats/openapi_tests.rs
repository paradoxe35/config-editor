//! OpenAPI documents as schema sources: one named schema is the document's
//! schema, 3.0 and Swagger keywords become 2020-12 ones, references point
//! into `$defs`, and what is not checked is a loss.

use serde_json::{Value, json};

use super::super::{LossEffect, SchemaFormat, SchemaInput};
use crate::editor::schema::Schema;

const API_30: &str = r##"{
  "openapi": "3.0.3",
  "info": {"title": "t", "version": "1"},
  "paths": {},
  "components": {"schemas": {
    "Server": {
      "type": "object",
      "properties": {
        "port": {"type": "integer", "nullable": true, "minimum": 0, "exclusiveMinimum": true, "example": 80},
        "host": {"type": "string", "xml": {"attribute": true}},
        "default": {"$ref": "#/components/schemas/Tags"}
      },
      "required": ["host"],
      "additionalProperties": false
    },
    "Tags": {"type": "array", "items": {"type": "string"}},
    "Pet": {"oneOf": [{"$ref": "#/components/schemas/Tags"}], "discriminator": {"propertyName": "kind"}}
  }}
}"##;

fn compile(text: &str, schema: Option<&str>) -> Result<Schema, String> {
    let mut options = serde_json::Map::new();
    if let Some(name) = schema {
        options.insert("schema".to_string(), Value::String(name.to_string()));
    }
    Schema::compile_as(
        SchemaFormat::OPENAPI,
        &SchemaInput::new(text).options(&options),
    )
    .map_err(|error| error.0)
}

#[test]
fn a_named_schema_of_an_openapi_3_0_document_is_the_schema() {
    let schema = compile(API_30, Some("Server")).unwrap();
    let json = schema.json();
    assert_eq!(json["$ref"], json!("#/$defs/Server"));
    let port = &json["$defs"]["Server"]["properties"]["port"];
    assert_eq!(port["type"], json!(["integer", "null"]));
    assert_eq!(port["exclusiveMinimum"], json!(0));
    assert!(port.get("minimum").is_none());
    assert_eq!(port["examples"], json!([80]));
    // A property named like a keyword is still a schema.
    assert_eq!(
        json["$defs"]["Server"]["properties"]["default"]["$ref"],
        json!("#/$defs/Tags")
    );
    assert_eq!(
        json["$defs"]["Server"]["properties"]["host"]["xml"],
        json!({"attribute": true})
    );
    let loss = schema
        .losses()
        .iter()
        .find(|loss| loss.construct == "discriminator")
        .expect("its loss");
    assert_eq!(loss.effect, LossEffect::Approximated);
}

/// The rewrite reads a schema's schemas as every walk does: a property
/// named `xml` or `discriminator` is a schema, OpenAPI's `xml`, `example`
/// and `discriminator` notes are data, and a discriminator's loss names
/// its place.
#[test]
fn openapi_notes_are_data_and_properties_named_like_them_are_schemas() {
    let api = r##"{"openapi": "3.0.3", "components": {"schemas": {
      "A": {"type": "object", "properties": {
        "xml": {"$ref": "#/components/schemas/B"},
        "discriminator": {"$ref": "#/components/schemas/B", "nullable": true},
        "c": {"type": "string", "xml": {"name": "c", "$ref": "#/components/schemas/B"}, "example": {"$ref": "#/components/schemas/B"}}
      }, "oneOf": [{"$ref": "#/components/schemas/B"}], "discriminator": {"propertyName": "kind", "mapping": {"b": "#/components/schemas/B"}}},
      "B": {"type": "integer"}
    }}}"##;
    let schema = compile(api, Some("A")).unwrap();
    let a = &schema.json()["$defs"]["A"];
    assert_eq!(a["properties"]["xml"]["$ref"], json!("#/$defs/B"));
    assert_eq!(
        a["properties"]["discriminator"]["anyOf"][0]["$ref"],
        json!("#/$defs/B")
    );
    assert_eq!(
        a["properties"]["c"]["xml"]["$ref"],
        json!("#/components/schemas/B"),
        "the XML object is data"
    );
    assert_eq!(
        a["properties"]["c"]["examples"],
        json!([{"$ref": "#/components/schemas/B"}])
    );
    assert_eq!(
        a["discriminator"]["mapping"]["b"],
        json!("#/components/schemas/B")
    );
    let places: Vec<&str> = schema
        .losses()
        .iter()
        .filter(|loss| loss.construct == "discriminator")
        .map(|loss| loss.message.as_str())
        .collect();
    assert_eq!(places.len(), 1, "{places:?}");
    assert!(
        places[0].starts_with("#/components/schemas/A: "),
        "{places:?}"
    );
}

#[cfg(all(feature = "schema-validation", feature = "yaml"))]
#[test]
fn an_openapi_schema_validates_documents() {
    use crate::editor::Language;
    let schema = compile(API_30, Some("Server")).unwrap();
    let problems = |text: &str| -> Vec<String> {
        schema
            .validate(Language::YAML, text)
            .into_iter()
            .map(|problem| problem.message)
            .collect()
    };
    assert!(problems("host: h\nport: null\n").is_empty());
    assert!(problems("host: h\nport: 1\ndefault: [a]\n").is_empty());
    assert_eq!(
        problems("host: h\nport: 0\n"),
        ["/port: 0 is less than or equal to the minimum of 0"]
    );
    assert_eq!(problems("port: 1\n"), ["\"host\" is a required property"]);
}

#[test]
fn swagger_and_openapi_3_1_documents_and_the_errors() {
    let swagger = r##"{"swagger": "2.0", "definitions": {"A": {"type": "object", "properties": {"b": {"$ref": "#/definitions/B"}}}, "B": {"type": "integer"}}}"##;
    let schema = compile(swagger, Some("A")).unwrap();
    assert_eq!(
        schema.json()["$defs"]["A"]["properties"]["b"]["$ref"],
        json!("#/$defs/B")
    );
    let single = r#"{"openapi": "3.1.0", "components": {"schemas": {"Only": {"type": ["integer", "null"], "exclusiveMinimum": 3}}}}"#;
    let schema = compile(single, None).unwrap();
    assert_eq!(schema.json()["$ref"], json!("#/$defs/Only"));
    assert_eq!(
        schema.json()["$defs"]["Only"]["exclusiveMinimum"],
        json!(3),
        "3.1 is 2020-12 already"
    );
    for (text, name, expected) in [
        (
            API_30,
            None,
            "say which schema with the option \"schema\" (one of Server, Tags, Pet)",
        ),
        (API_30, Some("Nope"), "the document has no schema \"Nope\""),
        ("{\"info\": {}}", None, "the document is no OpenAPI"),
        (
            r#"{"openapi": "3.0.0"}"#,
            None,
            "the document has no named schemas (components.schemas)",
        ),
    ] {
        let error = compile(text, name).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    assert_eq!(
        SchemaFormat::from_path("api/petstore.openapi.yaml"),
        Some(SchemaFormat::OPENAPI)
    );
    assert_eq!("openapi".parse::<SchemaFormat>(), Ok(SchemaFormat::OPENAPI));
}

#[cfg(feature = "yaml")]
#[test]
fn an_openapi_document_written_in_yaml() {
    let yaml = "openapi: 3.0.0\ncomponents:\n  schemas:\n    S:\n      type: string\n      nullable: true\n";
    let schema = compile(yaml, None).unwrap();
    assert_eq!(
        schema.json()["$defs"]["S"]["type"],
        json!(["string", "null"])
    );
}
