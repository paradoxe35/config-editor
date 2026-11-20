//! The least list a fill writes meets every list rule at the list
//! whenever some list the language writes does: a matrix over the rules
//! (`minItems`, `maxItems`, `prefixItems`, `items` true, false or typed,
//! `contains` with `minContains` and `maxContains`), each set given by one
//! schema and split between the property and an `allOf` member. Where the
//! plan finds no list, a search over short lists of plain values finds
//! none either.

use super::super::applies::Entry;
use super::super::completion::{language_keeps, least_list};
use super::Language;
use super::harness::{completing, report};
use serde_json::{Map, Value, json};

/// The values the search for a list meeting the rules builds lists from.
fn pool() -> [Value; 6] {
    [
        json!(0),
        json!("m"),
        json!("a"),
        Value::Null,
        json!([]),
        json!(1),
    ]
}

/// Longest list the search tries: the rules below never need more.
const LONGEST: u32 = 4;

/// Whether some list of [`pool`] values, at most [`LONGEST`] long, meets
/// `schema` and is written by `language` as itself.
fn some_list_meets(language: Language, schema: &Value) -> bool {
    let validator = jsonschema::validator_for(schema).expect("a valid schema");
    let pool = pool();
    let size = pool.len();
    (0..=LONGEST).any(|length| {
        (0..size.pow(length)).any(|mut index| {
            let mut items = Vec::new();
            for _ in 0..length {
                items.push(pool[index % size].clone());
                index /= size;
            }
            let list = Value::Array(items);
            validator.is_valid(&list) && language_keeps(language.behavior(), &list, schema, schema)
        })
    })
}

/// Every combination of the rules, as one schema and split.
fn schemas() -> Vec<Value> {
    let least = [None, Some(1), Some(2)];
    let most = [None, Some(1), Some(3)];
    let prefixes = [
        None,
        Some(json!([{"type": "integer"}])),
        Some(json!([{"type": "null"}])),
        Some(json!([{"type": "array"}])),
    ];
    let items = [
        None,
        Some(json!(true)),
        Some(json!(false)),
        Some(json!({"type": "string"})),
    ];
    let integer = json!({"type": "integer"});
    let contains = [
        None,
        Some((integer.clone(), None, None)),
        Some((json!({"const": "m"}), Some(2), None)),
        Some((integer.clone(), Some(0), Some(1))),
        Some((integer, None, Some(1))),
    ];
    let mut schemas = Vec::new();
    for least in least {
        for most in most {
            for prefix in &prefixes {
                for item in &items {
                    for contained in &contains {
                        for split in [false, true] {
                            let mut own = Map::new();
                            let mut member = Map::new();
                            own.insert("type".into(), json!("array"));
                            let asked = if split { &mut member } else { &mut own };
                            if let Some(least) = least {
                                asked.insert("minItems".into(), json!(least));
                            }
                            if let Some((schema, least, most)) = contained {
                                asked.insert("contains".into(), schema.clone());
                                if let Some(least) = least {
                                    asked.insert("minContains".into(), json!(least));
                                }
                                if let Some(most) = most {
                                    asked.insert("maxContains".into(), json!(most));
                                }
                            }
                            if let Some(most) = most {
                                own.insert("maxItems".into(), json!(most));
                            }
                            if let Some(prefix) = prefix {
                                own.insert("prefixItems".into(), prefix.clone());
                            }
                            if let Some(item) = item {
                                own.insert("items".into(), item.clone());
                            }
                            if split {
                                own.insert("allOf".into(), json!([member]));
                            }
                            schemas.push(Value::Object(own));
                        }
                    }
                }
            }
        }
    }
    schemas
}

#[test]
fn a_least_list_meets_every_list_rule_whenever_some_list_does() {
    let mut failures = Vec::new();
    let mut written = 0;
    for schema in schemas() {
        let validator = jsonschema::validator_for(&schema).expect("a valid schema");
        for language in completing() {
            match least_list(language.behavior(), &Entry::schema(&schema), &schema) {
                Some(list) if !validator.is_valid(&list) => {
                    failures.push(format!("{language} {schema}: writes {list}, refused"));
                }
                Some(_) => written += 1,
                // XML writes no list in a list nor an empty one, and a list
                // of one as its item: the search cannot tell its lists.
                None if language == Language::XML => {}
                None if some_list_meets(language, &schema) => {
                    failures.push(format!("{language} {schema}: none written, one meets it"));
                }
                None => {}
            }
        }
    }
    report(&failures);
    // Most combinations have a list: the matrix judges written lists.
    assert!(written > 2000, "{written} lists written");
}

/// The named shapes the plan reads as one: rules split between the
/// property and an `allOf` member, `items: false` beside `minItems`, a
/// tuple item refusing the `contains` item, a first item only `null`
/// meets, an item no schema says anything of.
#[test]
fn the_least_list_of_each_split_shape_is_the_fewest_items_meeting_all() {
    let cases = [
        (
            json!({"type": "array", "allOf": [{"minItems": 1, "contains": {"type": "integer"}}]}),
            Some(json!([0])),
        ),
        (
            json!({"type": "array", "allOf": [{"minItems": 1}], "items": false}),
            None,
        ),
        (
            json!({"allOf": [{"minItems": 1, "contains": {"type": "integer"}}], "items": false}),
            None,
        ),
        (
            json!({"allOf": [{"minItems": 1, "contains": {"type": "integer"}}],
                "prefixItems": [{"type": "array"}]}),
            Some(json!([[], 0])),
        ),
        (
            json!({"allOf": [{"minItems": 1, "contains": {"type": "integer"}}],
                "prefixItems": [{"type": "null"}]}),
            Some(json!([null, 0])),
        ),
        (
            json!({"minItems": 2, "prefixItems": [{"minimum": 0}]}),
            Some(json!([null, null])),
        ),
    ];
    let mut failures = Vec::new();
    for (schema, wanted) in cases {
        let found = least_list(Language::JSON.behavior(), &Entry::schema(&schema), &schema);
        if found != wanted {
            failures.push(format!("{schema}: {found:?}, not {wanted:?}"));
        }
    }
    // TOML writes no `null`: a first item only it meets leaves no list.
    let null_first = json!({"minItems": 1, "prefixItems": [{"type": "null"}],
        "contains": {"type": "integer"}});
    let toml = least_list(
        Language::TOML.behavior(),
        &Entry::schema(&null_first),
        &null_first,
    );
    if toml.is_some() {
        failures.push(format!("TOML {null_first}: {toml:?}"));
    }
    report(&failures);
}
