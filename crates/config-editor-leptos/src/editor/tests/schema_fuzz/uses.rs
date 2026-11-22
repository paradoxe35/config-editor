//! Find uses on random schemas and documents: the uses the engine finds
//! (`key_names::uses_in`) are exactly those a brute-force reading finds,
//! which walks the document beside the schema by hand (`properties`,
//! `additionalProperties`, `items`, every `anyOf` option, `propertyNames`)
//! and takes every value (and every key `propertyNames` reads) whose
//! schema there carries the keyword naming `/clients` and that is the
//! name; each use's range covers the name as written.
//!
//! The schemas mix the places a name is used (a value, list items, an
//! object's field, an `anyOf` option, an object's keys) with places it
//! never is (free text, `examples`, the keyword naming another map).

use serde_json::{Map, Value, json};

use super::random::Rng;
use super::runner::{Case, Property, Verdict, failure, language, run};
use crate::editor::key_names::{NameRef, uses_in_language};
use crate::editor::schema::build_schema;

const KEYWORD: &str = "x-config-editor-keys";

/// The names values are drawn from; `NAME` is searched for.
const NAMES: [&str; 3] = ["home", "work", "self"];
const NAME: &str = "home";

/// A string schema naming the map at `pointer`.
fn named(pointer: &str) -> Value {
    json!({"type": "string", KEYWORD: pointer})
}

/// A field's schema and a value for it.
fn field(rng: &mut Rng, depth: usize) -> (Value, Value) {
    let kinds = if depth > 1 { 7 } else { 9 };
    match rng.below(kinds) {
        0 => (
            named("/clients"),
            Value::String((*rng.pick(&NAMES)).to_string()),
        ),
        1 => {
            let items: Vec<Value> = (0..1 + rng.below(3))
                .map(|_| Value::String((*rng.pick(&NAMES)).to_string()))
                .collect();
            (
                json!({"type": "array", "items": named("/clients")}),
                Value::Array(items),
            )
        }
        2 => (
            json!({"anyOf": [{"type": "integer"}, named("/clients")]}),
            Value::String((*rng.pick(&NAMES)).to_string()),
        ),
        3 => (
            json!({"type": "string"}),
            Value::String((*rng.pick(&NAMES)).to_string()),
        ),
        4 => (
            json!({"type": "string", "examples": NAMES}),
            Value::String((*rng.pick(&NAMES)).to_string()),
        ),
        5 => (
            named("/groups"),
            Value::String((*rng.pick(&NAMES)).to_string()),
        ),
        6 => {
            let mut keys = Map::new();
            for key in NAMES {
                if rng.chance(60) {
                    keys.insert(key.to_string(), json!(rng.below(9)));
                }
            }
            (
                json!({
                    "type": "object",
                    "propertyNames": named("/clients"),
                    "additionalProperties": {"type": "integer"}
                }),
                Value::Object(keys),
            )
        }
        _ => object(rng, depth + 1),
    }
}

/// An object of a few fields, named `f0`, `f1`, ….
fn object(rng: &mut Rng, depth: usize) -> (Value, Value) {
    let (mut properties, mut values) = (Map::new(), Map::new());
    for index in 0..1 + rng.below(4) {
        let (schema, value) = field(rng, depth);
        properties.insert(format!("f{index}"), schema);
        values.insert(format!("f{index}"), value);
    }
    (
        json!({"type": "object", "properties": properties}),
        Value::Object(values),
    )
}

fn draw(rng: &mut Rng, index: usize) -> Case {
    let (mut schema, mut doc) = object(rng, 0);
    schema["properties"]["clients"] = json!({"type": "object"});
    doc["clients"] = json!({"home": {}, "work": {}});
    Case {
        language: language(index),
        pick: 0,
        schema,
        doc,
        other: Value::Null,
    }
}

/// Whether `schema` names `/clients` with the keyword, on itself or in an
/// `anyOf` option.
fn names_clients(schema: &Value) -> bool {
    let own = schema.get(KEYWORD) == Some(&json!("/clients"));
    let option = schema
        .get("anyOf")
        .and_then(Value::as_array)
        .is_some_and(|options| options.iter().any(names_clients));
    own || option
}

/// The uses of `NAME` in `value`, read beside `schema` by hand.
fn oracle(schema: &Value, value: &Value, pointer: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            let keys_named = schema.get("propertyNames").is_some_and(names_clients);
            for (key, member) in members {
                let at = format!("{pointer}/{key}");
                if keys_named && key == NAME {
                    found.push(at.clone());
                }
                let child = schema
                    .get("properties")
                    .and_then(|properties| properties.get(key))
                    .or_else(|| {
                        schema
                            .get("additionalProperties")
                            .filter(|it| it.is_object())
                    });
                if let Some(child) = child {
                    oracle(child, member, &at, found);
                }
            }
        }
        Value::Array(items) => {
            if let Some(item) = schema.get("items").filter(|it| it.is_object()) {
                for (index, member) in items.iter().enumerate() {
                    oracle(item, member, &format!("{pointer}/{index}"), found);
                }
            }
        }
        // A list a language reads back as its lone item (XML): the item.
        scalar if schema.get("items").is_some_and(Value::is_object) => {
            oracle(&schema["items"], scalar, pointer, found);
        }
        scalar => {
            let text = match scalar {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            if text == NAME && names_clients(schema) {
                found.push(pointer.to_string());
            }
        }
    }
}

pub(super) fn check(case: &Case) -> Verdict {
    let Ok(bundle) = build_schema(&case.schema.to_string()) else {
        return Verdict::Skipped;
    };
    let Ok(text) = case.language.serialize(&case.doc) else {
        return Verdict::Skipped;
    };
    let Ok(read) = case.language.parse(&text) else {
        return Verdict::Skipped;
    };
    let name = NameRef {
        schema: bundle.identity.clone(),
        pointer: "/clients".into(),
        name: NAME.into(),
    };
    let hits = uses_in_language(
        &text,
        case.language,
        &bundle.schema,
        &bundle.identity,
        &name,
    );
    let mut got: Vec<String> = hits.iter().map(|hit| hit.path.to_pointer()).collect();
    let mut want = Vec::new();
    oracle(&case.schema, &read, "", &mut want);
    got.sort();
    want.sort();
    if got != want {
        return Verdict::Fails(failure(
            "find uses differs from the brute-force reading",
            format!("got {got:?}\nwant {want:?}\n{text}"),
        ));
    }
    for hit in &hits {
        let written = &text[usize::from(hit.range.start())..usize::from(hit.range.end())];
        if written.trim_matches(|c| c == '"' || c == '\'') != NAME {
            return Verdict::Fails(failure(
                "a use's range does not cover the name",
                format!("{} covers {written:?}\n{text}", hit.path),
            ));
        }
    }
    Verdict::Holds
}

#[test]
fn find_uses_finds_exactly_what_a_brute_force_reading_finds() {
    run(&Property {
        name: "uses",
        check_path: "super::uses::check",
        cases: 300,
        shrink_budget: 1500,
        draw,
        check,
    });
}
