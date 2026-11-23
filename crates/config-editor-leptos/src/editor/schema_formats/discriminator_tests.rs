//! OpenAPI's `discriminator`: the property it names picks the schema a
//! value is checked by, beside `oneOf`/`anyOf` options (with a `mapping`
//! or by the options' names) or on a base schema its subtypes extend with
//! `allOf` (Swagger 2.0's polymorphism), in OpenAPI 3.0, 3.1 and Swagger
//! 2.0 alike; completion follows the option picked.

#![cfg(all(feature = "schema-validation", feature = "json"))]

use serde_json::{Value, json};

use super::{SchemaFormat, SchemaInput};
use crate::editor::Language;
use crate::editor::schema::Schema;

/// The pet schemas of the OpenAPI specification's example: `Cat` and
/// `Dog` extend `Pet` (`allOf`), which says the `petType`.
fn pets() -> Value {
    json!({
        "Pet": {"type": "object", "required": ["petType"],
            "properties": {"petType": {"type": "string"}, "name": {"type": "string"}}},
        "Cat": {"allOf": [{"$ref": "#/components/schemas/Pet"}, {"type": "object",
            "properties": {"huntingSkill": {"type": "string", "enum": ["clueless", "lazy"]}}}]},
        "Dog": {"allOf": [{"$ref": "#/components/schemas/Pet"}, {"type": "object",
            "properties": {"packSize": {"type": "integer", "minimum": 0}}}]},
    })
}

/// `schemas` as a document of `version` (`"2.0"`: Swagger, its references
/// into `definitions`), the schema `Owner` holding `pet`.
fn document(version: &str, mut schemas: Value, pet: Value) -> String {
    schemas["Owner"] = json!({"type": "object", "properties": {"pet": pet}});
    if version == "2.0" {
        json!({"swagger": "2.0", "definitions": schemas})
            .to_string()
            .replace("#/components/schemas/", "#/definitions/")
    } else {
        json!({"openapi": version, "components": {"schemas": schemas}}).to_string()
    }
}

fn compile(text: &str, name: &str) -> Schema {
    let mut options = serde_json::Map::new();
    options.insert("schema".into(), json!(name));
    Schema::compile_as(
        SchemaFormat::OPENAPI,
        &SchemaInput::new(text).options(&options),
    )
    .unwrap_or_else(|error| panic!("{}\n{text}", error.0))
}

fn problems(schema: &Schema, text: &str) -> Vec<String> {
    schema
        .validate(Language::JSON, text)
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

/// Each way of writing the pets' discriminator: the `pet` schema, the
/// value each option is picked by, and whether the base is extended
/// (`allOf`) rather than listed (`oneOf`).
fn shapes(version: &str) -> Vec<(&'static str, Value, [&'static str; 2])> {
    let options = |keyword: &str, mapping: bool| {
        let mut discriminator = json!({"propertyName": "petType"});
        if mapping {
            discriminator["mapping"] = json!({"cat": "#/components/schemas/Cat", "dog": "Dog"});
        }
        json!({keyword: [{"$ref": "#/components/schemas/Cat"}, {"$ref": "#/components/schemas/Dog"}],
            "discriminator": discriminator})
    };
    let mut shapes = vec![
        (
            "oneOf with a mapping",
            options("oneOf", true),
            ["cat", "dog"],
        ),
        (
            "oneOf by the options' names",
            options("oneOf", false),
            ["Cat", "Dog"],
        ),
        (
            "anyOf with a mapping",
            options("anyOf", true),
            ["cat", "dog"],
        ),
    ];
    if version == "2.0" {
        shapes.push((
            "a base its subtypes extend",
            json!({"$ref": "#/components/schemas/Pet"}),
            ["Cat", "Dog"],
        ));
    } else {
        shapes.push((
            "a base its subtypes extend, by name",
            json!({"$ref": "#/components/schemas/Pet"}),
            ["Cat", "Dog"],
        ));
    }
    shapes
}

/// The base schema `Pet` with its discriminator, for the shapes that put
/// it there.
fn with_base_discriminator(version: &str) -> Value {
    let mut schemas = pets();
    schemas["Pet"]["discriminator"] = if version == "2.0" {
        json!("petType")
    } else {
        json!({"propertyName": "petType"})
    };
    schemas
}

#[test]
fn the_discriminator_picks_the_option_a_value_is_checked_by() {
    let mut failures = Vec::new();
    for version in ["3.0.3", "3.1.0", "2.0"] {
        for (shape, pet, [cat, dog]) in shapes(version) {
            let schemas = if pet.get("$ref").is_some() {
                with_base_discriminator(version)
            } else {
                pets()
            };
            let text = document(version, schemas, pet);
            let schema = compile(&text, "Owner");
            let cases = [
                (json!({"petType": cat, "huntingSkill": "lazy"}), true),
                (json!({"petType": dog, "packSize": 3, "name": "rex"}), true),
                (json!({"petType": dog, "packSize": -1}), false),
                (json!({"petType": cat, "huntingSkill": "fast"}), false),
                (json!({"petType": "fish"}), false),
                (json!({"name": "rex"}), false),
                (json!({"petType": 1}), false),
            ];
            for (pet, valid) in cases {
                let value = json!({"pet": pet}).to_string();
                let found = problems(&schema, &value);
                if found.is_empty() != valid {
                    failures.push(format!(
                        "{version} {shape}: {value}: want valid {valid}, got {found:?}"
                    ));
                }
                // One problem, about the value, not a choice of options.
                if !valid && found.iter().any(|message| message.contains("options")) {
                    failures.push(format!("{version} {shape}: {value}: {found:?}"));
                }
            }
            if !schema.losses().is_empty() {
                failures.push(format!("{version} {shape}: losses {:?}", schema.losses()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Completion offers the discriminator's values, and once one is written
/// the keys of the option it picks.
#[test]
fn completion_offers_the_values_and_the_picked_options_keys() {
    let labels = |schema: &Schema, marked: &str| -> Vec<String> {
        // The caret's place in UTF-16 units: the text before the mark.
        let (before, _) = marked.split_once('|').unwrap();
        let text = marked.replace('|', "");
        let mut labels: Vec<String> = schema
            .suggestions(Language::JSON, &text, before.encode_utf16().count())
            .iter()
            .map(|option| option.label().to_string())
            .collect();
        labels.sort();
        labels
    };
    let mut failures = Vec::new();
    for version in ["3.0.3", "3.1.0", "2.0"] {
        for (shape, pet, [_, dog]) in shapes(version) {
            let schemas = if pet.get("$ref").is_some() {
                with_base_discriminator(version)
            } else {
                pets()
            };
            let schema = compile(&document(version, schemas, pet), "Owner");
            let picked = labels(
                &schema,
                &format!("{{\"pet\": {{\"petType\": \"{dog}\", \"|\"}}}}"),
            );
            for wanted in ["name", "packSize"] {
                if !picked.iter().any(|label| label == wanted) {
                    failures.push(format!(
                        "{version} {shape}: {wanted} not offered: {picked:?}"
                    ));
                }
            }
            if picked.iter().any(|label| label == "huntingSkill") {
                failures.push(format!(
                    "{version} {shape}: another option's key offered: {picked:?}"
                ));
            }
            let values = labels(&schema, "{\"pet\": {\"petType\": \"|\"}}");
            if !values.iter().any(|label| label.contains(dog)) {
                failures.push(format!("{version} {shape}: {dog} not offered: {values:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What a discriminator cannot say (an option written in place, which no
/// name or mapping picks) keeps the options as they are and is a loss; a
/// `discriminator` that is an extension's data (Kubernetes'
/// `x-kubernetes-unions`) is none; a value that is no object is checked
/// by the options.
#[test]
fn a_discriminator_it_cannot_express_is_a_loss_and_an_extensions_is_none() {
    let inline = json!({"oneOf": [{"$ref": "#/components/schemas/Cat"},
        {"type": "object", "properties": {"petType": {"const": "fish"}}}],
        "discriminator": {"propertyName": "petType"}});
    let schema = compile(&document("3.0.3", pets(), inline), "Owner");
    let losses: Vec<String> = schema.losses().iter().map(ToString::to_string).collect();
    assert_eq!(losses.len(), 1, "{losses:?}");
    assert!(
        losses[0].starts_with("#/components/schemas/Owner/properties/pet: "),
        "{losses:?}"
    );
    let union = json!({"type": "object", "properties": {"type": {"type": "string"}},
        "x-kubernetes-unions": [{"discriminator": "type", "fields-to-discriminateBy": {"a": "A"}}]});
    let schema = compile(&document("2.0", pets(), union), "Owner");
    assert!(schema.losses().is_empty(), "{:?}", schema.losses());
    let options = json!({"oneOf": [{"$ref": "#/components/schemas/Count"}],
        "discriminator": {"propertyName": "kind", "mapping": {"n": "Count"}}});
    let mut schemas = pets();
    schemas["Count"] = json!({"type": "integer"});
    let schema = compile(&document("3.1.0", schemas, options), "Owner");
    assert!(problems(&schema, "{\"pet\": 3}").is_empty());
    assert!(!problems(&schema, "{\"pet\": \"x\"}").is_empty());
}
