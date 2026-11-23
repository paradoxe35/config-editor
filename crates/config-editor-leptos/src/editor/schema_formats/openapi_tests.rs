//! OpenAPI documents as schema sources: one named schema is the document's
//! schema, 3.0 and Swagger keywords become 2020-12 ones, references point
//! into `$defs`, and what is not checked is a loss.

use serde_json::{Value, json};

use super::super::{SchemaFormat, SchemaInput};
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
    // `Pet`, which `Server` never reaches, is left out.
    assert!(json["$defs"].get("Pet").is_none());
    assert!(schema.losses().is_empty(), "{:?}", schema.losses());
}

/// The rewrite reads a schema's schemas as every walk does: a property
/// named `xml` or `discriminator` is a schema, OpenAPI's `xml`, `example`
/// and `discriminator` notes are data (a discriminator then picks its
/// option), and the loss of one it cannot express names its place.
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
    assert!(a.get("discriminator").is_none(), "{a}");
    assert_eq!(
        a["allOf"][0]["then"]["allOf"][0]["then"]["$ref"],
        json!("#/$defs/B")
    );
    assert_eq!(
        a["allOf"][0]["else"]["oneOf"][0]["$ref"],
        json!("#/$defs/B")
    );
    let inline = api.replace(
        r##""oneOf": [{"$ref": "#/components/schemas/B"}]"##,
        r##""oneOf": [{"type": "integer"}]"##,
    );
    let schema = compile(&inline, Some("A")).unwrap();
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
        (
            "{\"info\": {}}",
            None,
            "the document is not an OpenAPI or Swagger document",
        ),
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

/// Every JSON Schema keyword OpenAPI 3.0 or Swagger 2.0 does not define
/// (`contains` and its bounds, `prefixItems`, `const`, `if`, the
/// dependent and unevaluated keywords, `patternProperties`,
/// `propertyNames`; Swagger also lacks `oneOf`, `anyOf` and `not`) is
/// kept as written and means what it means in JSON Schema: such a
/// document checks a value as the plain schema does, in each format.
#[cfg(all(feature = "schema-validation", feature = "json"))]
#[test]
fn keywords_openapi_3_0_and_swagger_lack_are_kept_and_checked_as_json_schema() {
    use crate::editor::Language;
    let cases = [
        (
            json!({"minItems": 1, "contains": {"type": "integer", "minimum": 0}}),
            "[0]",
            "[\"a\"]",
        ),
        (
            json!({"contains": {"type": "integer"}, "minContains": 2, "maxContains": 2}),
            "[1, 2]",
            "[1]",
        ),
        (
            json!({"prefixItems": [{"type": "string"}], "items": false}),
            "[\"a\"]",
            "[\"a\", 1]",
        ),
        (json!({"const": 3}), "3", "4"),
        (
            json!({"if": {"type": "integer"}, "then": {"minimum": 1}, "else": {"type": "string"}}),
            "2",
            "0",
        ),
        (
            json!({"dependentRequired": {"a": ["b"]}}),
            "{\"a\": 1, \"b\": 2}",
            "{\"a\": 1}",
        ),
        (
            json!({"dependentSchemas": {"a": {"required": ["b"]}}}),
            "{\"a\": 1, \"b\": 2}",
            "{\"a\": 1}",
        ),
        (
            json!({"properties": {"a": {}}, "unevaluatedProperties": false}),
            "{\"a\": 1}",
            "{\"b\": 1}",
        ),
        (
            json!({"prefixItems": [{}], "unevaluatedItems": false}),
            "[1]",
            "[1, 2]",
        ),
        (
            json!({"patternProperties": {"^x": {"type": "integer"}}}),
            "{\"xa\": 1}",
            "{\"xa\": \"s\"}",
        ),
        (
            json!({"propertyNames": {"maxLength": 2}}),
            "{\"ab\": 1}",
            "{\"abc\": 1}",
        ),
        (
            json!({"oneOf": [{"type": "integer"}, {"minimum": 0}]}),
            "-1",
            "1",
        ),
        (
            json!({"anyOf": [{"type": "integer"}, {"type": "boolean"}]}),
            "true",
            "\"s\"",
        ),
        (json!({"not": {"type": "string"}}), "1", "\"s\""),
    ];
    for (keyword_schema, good, bad) in cases {
        let plain = Schema::compile(&keyword_schema.to_string()).unwrap();
        for document in [
            json!({"openapi": "3.0.3", "components": {"schemas": {"Root": keyword_schema}}}),
            json!({"swagger": "2.0", "definitions": {"Root": keyword_schema}}),
        ] {
            let through = compile(&document.to_string(), None).unwrap();
            assert_eq!(
                through.json()["$defs"]["Root"],
                keyword_schema,
                "{document}: kept as written"
            );
            let messages = |schema: &Schema, text: &str| -> Vec<String> {
                let problems = schema.validate(Language::JSON, text);
                problems
                    .into_iter()
                    .map(|problem| problem.message)
                    .collect()
            };
            for text in [good, bad] {
                assert_eq!(
                    messages(&through, text),
                    messages(&plain, text),
                    "{document}: {text}"
                );
            }
            assert!(messages(&through, good).is_empty(), "{document}: {good}");
            assert!(!messages(&through, bad).is_empty(), "{document}: {bad}");
        }
    }
}

/// Only the named schema and what it refers to are compiled: a schema it
/// never reaches is left out, with its losses (a discriminator, Swagger's
/// `type: file`) and whatever no JSON Schema validator takes in it.
#[test]
fn only_what_the_named_schema_reaches_is_compiled_and_told() {
    let swagger = r##"{"swagger": "2.0", "definitions": {
      "Pet": {"type": "object", "properties": {"tag": {"$ref": "#/definitions/Tag"}}},
      "Tag": {"type": "string"},
      "Upload": {"type": "object", "properties": {"f": {"type": "file"}}},
      "Union": {"oneOf": [{"type": "object"}], "discriminator": {"propertyName": "k"}}
    }}"##;
    let schema = compile(swagger, Some("Pet")).unwrap();
    let defs: Vec<&String> = schema.json()["$defs"].as_object().unwrap().keys().collect();
    assert_eq!(defs, ["Pet", "Tag"]);
    assert!(schema.losses().is_empty(), "{:?}", schema.losses());
    // Reached, `type: file` is a string, and its loss.
    let schema = compile(swagger, Some("Upload")).unwrap();
    assert_eq!(
        schema.json()["$defs"]["Upload"]["properties"]["f"]["type"],
        json!("string")
    );
    let losses: Vec<String> = schema.losses().iter().map(ToString::to_string).collect();
    assert_eq!(
        losses,
        ["#/definitions/Upload/properties/f: type: file is checked as a string"]
    );
    // An anchor a reference names may be anywhere: nothing is left out.
    let anchored = r##"{"openapi": "3.1.0", "components": {"schemas": {
      "A": {"$ref": "#port"}, "B": {"$anchor": "port", "type": "integer"}}}}"##;
    let schema = compile(anchored, Some("A")).unwrap();
    assert!(schema.json()["$defs"].get("B").is_some());
}

/// Swagger's `x-nullable: true` (as OpenAPI 3.0's `nullable`) lets a
/// value be `null`.
#[cfg(all(feature = "schema-validation", feature = "json"))]
#[test]
fn x_nullable_lets_a_swagger_value_be_null() {
    use crate::editor::Language;
    for (version, keyword) in [
        ("2.0", "x-nullable"),
        ("3.0.3", "x-nullable"),
        ("3.0.3", "nullable"),
    ] {
        let schemas = json!({"Pet": {"type": "object", "properties": {
            "tag": {"type": "string", keyword: true},
            "kind": {"$ref": "#/components/schemas/Kind", keyword: true}}},
            "Kind": {"type": "string"}});
        let text = if version == "2.0" {
            json!({"swagger": version, "definitions": schemas})
                .to_string()
                .replace("#/components/schemas/", "#/definitions/")
        } else {
            json!({"openapi": version, "components": {"schemas": schemas}}).to_string()
        };
        let schema = compile(&text, Some("Pet")).unwrap();
        for (document, valid) in [
            (r#"{"tag": null, "kind": null}"#, true),
            (r#"{"tag": "a", "kind": "b"}"#, true),
            (r#"{"tag": 1}"#, false),
        ] {
            let problems = schema.validate(Language::JSON, document);
            assert_eq!(
                problems.is_empty(),
                valid,
                "{version} {keyword} {document}: {problems:?}"
            );
        }
    }
}
