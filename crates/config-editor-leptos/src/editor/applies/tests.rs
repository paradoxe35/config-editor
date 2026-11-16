//! The applicability model on its own: what applies where for which
//! document, and what one reading costs.

use super::*;
use crate::editor::work_count::{Work, counted};
use serde_json::json;

fn key(name: &str) -> SchemaPathSegment {
    SchemaPathSegment::Key(name.to_string())
}

/// The `type`s of the schemas offered at `path` of `document`.
fn types_at(schema: &Value, path: &[SchemaPathSegment], document: &Value) -> Vec<String> {
    let read = || Some(document);
    Applies::new(schema, false, &read)
        .at(path)
        .offered()
        .iter()
        .filter_map(|branch| branch.get("type").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[test]
fn every_applicator_applies_at_a_path() {
    // The base property only describes the field; its type comes from
    // `allOf`, a `$ref` in it, the `then` taken and a dependent schema.
    let schema = json!({
        "$defs": {"port": {"properties": {"port": {"type": "integer"}}}},
        "properties": {"port": {"description": "the port"}, "kind": {}},
        "allOf": [{"$ref": "#/$defs/port"}],
        "if": {"properties": {"kind": {"const": "a"}}},
        "then": {"properties": {"port": {"minimum": 1, "type": "number"}}},
        "else": {"properties": {"port": {"type": "string"}}},
        "dependentSchemas": {"tls": {"properties": {"port": {"type": "null"}}}}
    });
    let types = |document: Value| types_at(&schema, &[key("port")], &document);
    assert_eq!(types(json!({"kind": "a"})), ["integer", "number"]);
    assert_eq!(types(json!({"kind": "b"})), ["integer", "string"]);
    assert_eq!(
        types(json!({"kind": "a", "tls": 1})),
        ["integer", "number", "null"]
    );
    // Without a document both branches and every dependent schema apply.
    let blind = Applies::blind(&schema, false);
    let all: Vec<&str> = blind
        .at(&[key("port")])
        .branches()
        .iter()
        .filter_map(|branch| branch.get("type").and_then(Value::as_str))
        .collect();
    assert_eq!(all, ["integer", "number", "string", "null"]);
}

#[test]
fn options_are_narrowed_by_the_object() {
    let schema = json!({
        "oneOf": [
            {"title": "A", "required": ["kind"], "properties": {"kind": {"const": "a"}, "x": {"type": "string"}}},
            {"title": "B", "required": ["kind"], "properties": {"kind": {"const": "b"}, "x": {"type": "integer"}}},
            {"type": "string"}
        ]
    });
    assert_eq!(
        types_at(&schema, &[key("x")], &json!({"kind": "a"})),
        ["string"]
    );
    assert_eq!(
        types_at(&schema, &[key("x")], &json!({"kind": "b"})),
        ["integer"]
    );
    // A value no option lists leaves them all.
    assert_eq!(
        types_at(&schema, &[key("x")], &json!({"kind": "c"})),
        ["string", "integer"]
    );
    let read = || None;
    let applies = Applies::new(&schema, false, &read);
    let keys: HashSet<String> = ["kind".to_string()].into();
    let object = applies.at_object(&[], &keys);
    assert_eq!(
        object.choices().choices.len(),
        2,
        "the string option is no object's"
    );
}

#[test]
fn an_object_is_in_the_one_option_its_values_fit() {
    let schema = json!({"anyOf": [
        {"properties": {"v": {"type": "integer"}, "a1": {}}},
        {"properties": {"v": {"type": "boolean"}, "a2": {}}}
    ]});
    let keys = |document: Value| -> Vec<String> {
        Here::walk(
            Entry::schema(&schema),
            &schema,
            Seen::value(Some(&document)),
        )
        .offered()
        .iter()
        .filter_map(|branch| branch.get("properties").and_then(Value::as_object))
        .flat_map(|properties| properties.keys().cloned())
        .filter(|key| key != "v")
        .collect()
    };
    assert_eq!(keys(json!({"v": 3})), ["a1"]);
    assert_eq!(keys(json!({"v": true})), ["a2"]);
    assert_eq!(keys(json!({"v": "q"})), ["a1", "a2"]);
    // XML's text reads as the number it writes.
    assert_eq!(keys(json!({"v": "3"})), ["a1"]);
}

#[test]
fn a_schema_met_twice_in_a_step_keeps_its_applicators() {
    // Both options name one definition whose type is in its `allOf`.
    let schema = json!({
        "$defs": {"d": {"allOf": [{"properties": {"v": {"type": "integer"}}}]}},
        "anyOf": [
            {"$ref": "#/$defs/d", "properties": {"k": {"const": 1}}},
            {"$ref": "#/$defs/d", "properties": {"k": {"const": 2}}}
        ]
    });
    assert_eq!(
        types_at(&schema, &[key("v")], &json!({"k": 2})),
        ["integer"]
    );
}

#[test]
fn cycles_end_and_chains_of_any_length_are_followed() {
    let mut definitions = serde_json::Map::new();
    for link in 0..1000 {
        definitions.insert(
            format!("d{link}"),
            json!({"$ref": format!("#/$defs/d{}", link + 1)}),
        );
    }
    definitions.insert("d1000".into(), json!({"type": "integer"}));
    let schema = json!({"$defs": definitions, "properties": {"a": {"$ref": "#/$defs/d0"}}});
    assert_eq!(types_at(&schema, &[key("a")], &json!({})), ["integer"]);
    let cycle = json!({
        "$defs": {"a": {"$ref": "#/$defs/b"}, "b": {"$ref": "#/$defs/a", "title": "B"}},
        "properties": {"c": {"$ref": "#/$defs/a"}}
    });
    let read = || None;
    let titles: Vec<&str> = Applies::new(&cycle, false, &read)
        .at(&[key("c")])
        .branches()
        .iter()
        .filter_map(|branch| branch.get("title").and_then(Value::as_str))
        .collect();
    assert_eq!(titles, ["B"]);
}

/// A recursive `anyOf` whose options share a key: each level of
/// a path is one step, whatever the number of options.
fn walks_at_depth(depth: usize) -> usize {
    let schema = json!({
        "$defs": {"node": {"anyOf": [
            {"properties": {"c": {"$ref": "#/$defs/node"}, "a1": {}, "v": {"anyOf": [{"type": "integer"}, {"type": "string"}]}}},
            {"properties": {"c": {"$ref": "#/$defs/node"}, "a2": {}, "v": {"anyOf": [{"type": "boolean"}, {"type": "string"}]}}}
        ]}},
        "$ref": "#/$defs/node"
    });
    let path: Vec<SchemaPathSegment> = (0..depth).map(|_| key("c")).collect();
    let (_, walks) = counted(Work::BranchWalk, || {
        Applies::blind(&schema, false).at(&path).branches().len()
    });
    walks
}

#[test]
fn a_path_under_recursive_options_costs_one_step_per_level() {
    let (ten, twenty, forty) = (walks_at_depth(10), walks_at_depth(20), walks_at_depth(40));
    assert_eq!(
        forty - twenty,
        2 * (twenty - ten),
        "linear: {ten} {twenty} {forty}"
    );
}

/// A keyword of one kind of value (a list's, an object's) says nothing of
/// a value of another kind, and lists and objects are checked by it.
#[test]
fn keywords_of_one_kind_of_value_pass_every_other_kind() {
    let root = json!({});
    let cases = [
        (
            json!({"contains": {"const": "a"}, "items": {}}),
            json!("f"),
            Some(true),
        ),
        (
            json!({"contains": {"const": "a"}}),
            json!(["b", "a"]),
            Some(true),
        ),
        (
            json!({"contains": {"const": "a"}}),
            json!(["b"]),
            Some(false),
        ),
        (
            json!({"contains": {"const": "a"}, "minContains": 2}),
            json!(["a", "b"]),
            Some(false),
        ),
        (
            json!({"contains": {"const": "a"}, "minContains": 0}),
            json!([]),
            Some(true),
        ),
        (
            json!({"contains": {"const": "a"}, "maxContains": 1}),
            json!(["a", "a"]),
            Some(false),
        ),
        (json!({"uniqueItems": true}), json!(3), Some(true)),
        (json!({"uniqueItems": true}), json!([1, 1]), None),
        (
            json!({"propertyNames": {"maxLength": 1}}),
            json!({"ab": 1}),
            Some(false),
        ),
        (
            json!({"propertyNames": {"maxLength": 1}}),
            json!("ab"),
            Some(true),
        ),
        (
            json!({"dependentRequired": {"a": ["b"]}}),
            json!({"a": 1}),
            Some(false),
        ),
        (
            json!({"dependentRequired": {"a": ["b"]}}),
            json!({"a": 1, "b": 2}),
            Some(true),
        ),
        (json!({"unevaluatedItems": false}), json!({}), Some(true)),
        (
            json!({"unevaluatedProperties": false}),
            json!([]),
            Some(true),
        ),
        (
            json!({"unevaluatedProperties": false}),
            json!({"a": 1}),
            None,
        ),
    ];
    for (schema, value, expected) in cases {
        assert_eq!(
            decides(&schema, &value, &root),
            expected,
            "{schema} for {value}"
        );
    }
}

#[test]
fn draft_07_refs_stand_alone_when_values_are_checked() {
    let root = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": {"n": {"type": "integer"}}
    });
    let beside = json!({"$ref": "#/$defs/n", "minimum": 10});
    assert_eq!(decides(&beside, &json!(3), &root), Some(true));
    let later = json!({"$defs": {"n": {"type": "integer"}}});
    assert_eq!(decides(&beside, &json!(3), &later), Some(false));
}

#[test]
fn values_are_found_at_their_path() {
    let document = json!({"a": {"b": [{"c": 1}]}, "x": {"y": 2}});
    let item = SchemaPathSegment::ArrayItem(0);
    assert_eq!(
        value_at(&document, &[key("a"), key("b"), item.clone(), key("c")]),
        Some(&json!(1))
    );
    assert_eq!(
        value_at(&document, &[key("x"), item, key("y")]),
        Some(&json!(2)),
        "a lone value is its own first item"
    );
    assert_eq!(value_at(&document, &[key("z")]), None);
}

#[test]
fn a_lone_xml_value_is_one_item_once() {
    use crate::editor::completion::conform_arrays;
    use crate::editor::language::Repetition;
    let schema = json!({"properties": {"a": {"allOf": [{"type": "array"}]}}});
    let mut value = json!({"a": true});
    conform_arrays(&mut value, &schema, Repetition::Elements);
    assert_eq!(value, json!({"a": [true]}));
}
