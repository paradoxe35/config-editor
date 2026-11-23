//! Schemas whose options a property's value picks, for the formats
//! suite: the plain schema is the JSON Schema an OpenAPI `discriminator`
//! compiles to (an `if`/`then` per option on the property, inside an `if`
//! the value is an object, the options themselves for any other value),
//! and [`as_discriminator`] writes it back as the `oneOf`/`anyOf` and
//! `discriminator` (with a `mapping`, or by the options' names) a
//! document says. The suite then checks the format reads it as the plain
//! schema.

use serde_json::{Map, Value, json};

use super::random::Rng;
use super::runner::{Case, language};
use crate::editor::schema::each_schema_mut;

/// Option names, and the property's other values.
const NAMES: &[&str] = &["Cat", "Dog", "Fish", "Bird"];
const OTHER: &[&str] = &["cat", "dog", "x-1", "fish"];
/// The fields an option may have, with a value it takes and one it does
/// not.
const FIELDS: &[(&str, &str, &str, &str)] = &[
    ("size", r#"{"type": "integer", "minimum": 0}"#, "3", "-1"),
    (
        "skill",
        r#"{"enum": ["lazy", "keen"]}"#,
        r#""lazy""#,
        r#""fast""#,
    ),
    ("tame", r#"{"type": "boolean"}"#, "true", r#""no""#),
    (
        "name",
        r#"{"type": "string", "maxLength": 4}"#,
        r#""rex""#,
        r#""rexxxx""#,
    ),
];

/// A case: a schema holding a picked choice at `pet`, and a document.
pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let property = *rng.pick(&["petType", "kind"]);
    let count = 2 + rng.below(NAMES.len() - 1);
    let mapped = rng.chance(50);
    let mut defs = Map::new();
    let mut cases: Vec<(String, Vec<String>, usize)> = Vec::new();
    for (at, name) in NAMES.iter().take(count).enumerate() {
        let field = rng.below(FIELDS.len());
        let (key, schema, _, _) = FIELDS[field];
        let mut def = json!({"type": "object", "properties": {
            property: {"type": "string"},
            key: serde_json::from_str::<Value>(schema).unwrap_or_default(),
        }});
        if rng.chance(30) {
            def["required"] = json!([key]);
        }
        defs.insert((*name).to_string(), def);
        let values = if mapped {
            let mut values = vec![OTHER[at].to_string()];
            if rng.chance(25) {
                values.push(format!("{}2", OTHER[at]));
            }
            values
        } else {
            vec![(*name).to_string()]
        };
        cases.push(((*name).to_string(), values, field));
    }
    let keyword = if rng.chance(70) { "oneOf" } else { "anyOf" };
    let options: Vec<Value> = cases
        .iter()
        .map(|(name, _, _)| json!({"$ref": format!("#/$defs/{name}")}))
        .collect();
    let values: Vec<&String> = cases.iter().flat_map(|(_, values, _)| values).collect();
    let branches: Vec<Value> = cases
        .iter()
        .map(|(name, values, _)| {
            let value = match values.as_slice() {
                [one] => json!({"const": one}),
                many => json!({"enum": many}),
            };
            json!({"if": {"properties": {property: value}, "required": [property]},
                "then": {"$ref": format!("#/$defs/{name}")}})
        })
        .collect();
    let then = json!({"required": [property], "properties": {property: {"enum": values}},
        "allOf": branches});
    let pet =
        json!({"allOf": [{"if": {"type": "object"}, "then": then, "else": {keyword: options}}]});
    let schema = json!({"type": "object", "properties": {"pet": pet}, "$defs": defs});
    let doc = document(rng, property, &cases);
    Case {
        language: language(index),
        pick: 0,
        schema,
        doc,
        other: Value::Null,
    }
}

/// A document for the cases: the property naming an option (or another
/// value, or none), with a field of that option or another, its value
/// taken or not.
fn document(rng: &mut Rng, property: &str, cases: &[(String, Vec<String>, usize)]) -> Value {
    if rng.chance(5) {
        return json!({"pet": 3});
    }
    let (_, values, field) = &cases[rng.below(cases.len())];
    let mut pet = Map::new();
    match rng.below(10) {
        0 => {}
        1 => {
            pet.insert(property.to_string(), json!("zebra"));
        }
        _ => {
            pet.insert(property.to_string(), json!(rng.pick(values)));
        }
    }
    let field = if rng.chance(20) {
        rng.below(FIELDS.len())
    } else {
        *field
    };
    let (key, _, good, bad) = FIELDS[field];
    if rng.chance(85) {
        let text = if rng.chance(70) { good } else { bad };
        pet.insert(
            key.to_string(),
            serde_json::from_str(text).unwrap_or_default(),
        );
    }
    json!({ "pet": pet })
}

/// `schema` with each picked choice [`draw`] writes as a `discriminator`
/// beside its options, the mapping naming its schemas as `name` writes
/// them; false when the document cannot say it (Swagger's discriminator,
/// a property's name, has no mapping).
pub(super) fn as_discriminator(schema: &mut Value, swagger: bool) -> bool {
    let mut ok = true;
    each_schema_mut(schema, &mut |map| {
        let Some(Value::Array(all)) = map.get("allOf") else {
            return;
        };
        let Some(index) = all.iter().position(picked) else {
            return;
        };
        let item = all[index].clone();
        let Some((keyword, options)) = item["else"]
            .as_object()
            .and_then(|other| other.iter().next())
            .map(|(keyword, options)| (keyword.clone(), options.clone()))
        else {
            return;
        };
        let then = &item["then"];
        let property = then["required"][0].as_str().unwrap_or_default().to_string();
        let mut mapping = Map::new();
        let mut implicit = true;
        for branch in then["allOf"].as_array().into_iter().flatten() {
            let target = branch["then"]["$ref"].as_str().unwrap_or_default();
            let name = target.trim_start_matches("#/$defs/");
            let test = &branch["if"]["properties"][property.as_str()];
            let values: Vec<&Value> = match test.get("const") {
                Some(one) => vec![one],
                None => test["enum"].as_array().into_iter().flatten().collect(),
            };
            for value in values {
                let value = value.as_str().unwrap_or_default();
                implicit &= value == name;
                mapping.insert(value.to_string(), json!(name));
            }
        }
        let discriminator = match (implicit, swagger) {
            (true, true) => json!(property),
            (true, false) => json!({"propertyName": property}),
            (false, false) => json!({"propertyName": property, "mapping": mapping}),
            (false, true) => {
                ok = false;
                return;
            }
        };
        if let Some(Value::Array(all)) = map.get_mut("allOf") {
            all.remove(index);
            if all.is_empty() {
                map.remove("allOf");
            }
        }
        map.insert(keyword, options);
        map.insert("discriminator".to_string(), discriminator);
    });
    ok
}

/// Whether `item` (of an `allOf`) is a picked choice as [`draw`] writes it.
fn picked(item: &Value) -> bool {
    item.pointer("/if/type") == Some(&json!("object"))
        && item.pointer("/then/allOf").is_some()
        && item
            .get("else")
            .and_then(Value::as_object)
            .is_some_and(|other| {
                other.len() == 1 && (other.contains_key("oneOf") || other.contains_key("anyOf"))
            })
}
