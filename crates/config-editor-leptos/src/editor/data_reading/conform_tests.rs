//! XML text and lone elements are read by the kinds every schema at them
//! allows together: an `allOf` narrowing a wider `type`, or the `then`
//! an object takes, decides as a `type` beside the field does.

use super::super::conform_arrays;
use crate::editor::language::Repetition;
use serde_json::{Value, json};

fn read(schema: &Value, mut value: Value) -> Value {
    conform_arrays(&mut value, schema, Repetition::Elements);
    value
}

#[test]
fn text_reads_as_the_kind_all_schemas_allow() {
    // The base allows text, the `allOf` only an integer.
    let narrowed = json!({"properties": {"port": {"type": ["integer", "string"]}},
        "allOf": [{"properties": {"port": {"type": "integer"}}}]});
    assert_eq!(read(&narrowed, json!({"port": "8"})), json!({"port": 8}));
    // The `then` every object takes gives the type.
    let conditional = json!({"properties": {"port": {"type": ["integer", "string"]}},
        "if": {"type": "object"}, "then": {"properties": {"port": {"type": "integer"}}}});
    assert_eq!(read(&conditional, json!({"port": "8"})), json!({"port": 8}));
    // Either kind still allowed: the text stays text.
    let wide = json!({"properties": {"port": {"type": ["integer", "string"]}}});
    assert_eq!(read(&wide, json!({"port": "8"})), json!({"port": "8"}));
    // An empty element where the base allows text and the `allOf` only
    // `null`: `null`, not the empty text.
    let null = json!({"properties": {"v": {"type": ["string", "null"]}},
        "allOf": [{"properties": {"v": {"type": "null"}}}]});
    assert_eq!(read(&null, json!({"v": {}})), json!({"v": null}));
}

#[test]
fn an_empty_element_reads_as_the_empty_value_the_schema_takes() {
    // Text is allowed only as a listed word, `null` as its type: an empty
    // element is `null`, not the empty text the list refuses.
    for schema in [
        json!({"properties": {"v": {"anyOf": [{"type": "null"}, {"enum": ["auto"]}]}}}),
        json!({"properties": {"v": {"type": ["null", "string"], "minLength": 1}}}),
    ] {
        assert_eq!(
            read(&schema, json!({"v": {}})),
            json!({"v": null}),
            "{schema}"
        );
    }
    // Text the schema takes empty stays the empty text.
    let text = json!({"properties": {"v": {"type": ["null", "string"]}}});
    assert_eq!(read(&text, json!({"v": {}})), json!({"v": ""}));
}

#[test]
fn a_lone_element_is_a_list_where_all_schemas_want_one() {
    let narrowed = json!({"properties": {"dns": {"type": ["string", "array"]}},
        "allOf": [{"properties": {"dns": {"type": "array", "items": {"type": "string"}}}}]});
    assert_eq!(
        read(&narrowed, json!({"dns": "1.1.1.1"})),
        json!({"dns": ["1.1.1.1"]})
    );
    let wide = json!({"properties": {"dns": {"type": ["string", "array"]}}});
    assert_eq!(
        read(&wide, json!({"dns": "1.1.1.1"})),
        json!({"dns": "1.1.1.1"})
    );
}

/// A lone element a schema names a list for is the list's one item where
/// the object takes it so, though an option silent on it leaves it to one
/// typing it otherwise (so no kind is allowed there together).
#[test]
fn a_lone_element_is_a_list_where_an_option_typing_it_otherwise_cannot_be_taken() {
    let typed = |options: Value| {
        json!({"properties": {"mode": {"allOf": [options,
            {"required": ["size"], "properties": {"size": {"type": "array", "minItems": 1}}}]}}})
    };
    let others = [
        json!({"additionalProperties": {"type": "string"}}),
        json!({"patternProperties": {"^s": {"type": "integer"}}}),
        json!({"properties": {"size": {"type": "boolean"}}}),
    ];
    for other in others {
        for options in [json!({"anyOf": [other, {}]}), json!({"oneOf": [{}, other]})] {
            let schema = typed(options);
            assert_eq!(
                read(&schema, json!({"mode": {"size": 0}})),
                json!({"mode": {"size": [0]}}),
                "{schema}"
            );
        }
    }
}

/// A lone element one option names a list for, where that list makes the
/// object match more options than `oneOf` lets it (or one a `not`
/// refuses), is the value written: the list's reading is kept only where
/// the schemas above it take it, at any depth.
#[test]
fn a_lone_element_is_no_list_where_the_object_above_then_refuses_it() {
    let list = json!({"properties": {"a": {"type": "array"}}});
    let under = |depth: usize, schema: Value| {
        (0..depth).fold(schema, |inner, _| json!({"properties": {"p": inner}}))
    };
    let doc = |depth: usize, inner: Value| {
        (0..depth).fold(json!({"a": inner}), |inner, _| json!({"p": inner}))
    };
    let lone = |depth: usize| doc(depth, json!("x"));
    let listed = |depth: usize| doc(depth, json!(["x"]));
    for depth in 0..3 {
        // Both options would match the list: the text alone.
        let one_of = json!({"oneOf": [list, {}]});
        assert_eq!(read(&under(depth, one_of), lone(depth)), lone(depth));
        let above = json!({"oneOf": [under(depth, list.clone()), {}]});
        assert_eq!(read(&above, lone(depth)), lone(depth));
        let not = json!({"properties": {"a": {"type": ["array", "string"]}}, "not": list});
        assert_eq!(read(&under(depth, not), lone(depth)), lone(depth));
        // Only the list meets one option: the list.
        let only = json!({"oneOf": [list, {"properties": {"a": {"type": "integer"}}}]});
        assert_eq!(read(&under(depth, only), lone(depth)), listed(depth));
        // Any option may match: the list an option names.
        let any_of = json!({"anyOf": [list, {}]});
        assert_eq!(read(&under(depth, any_of), lone(depth)), listed(depth));
    }
}

/// A lone element whose text an object option lists (`<a><b>1.5</b></a>`,
/// `b: enum ["1.5"]`) beside an option for a list is the object, its text
/// read as listed, not a list of one: the value as written is tried (with
/// its text read as the schemas want it) before a list's one item. Each
/// reading is one the schema takes; elements written twice stay the list
/// written.
#[test]
fn a_lone_element_an_object_option_takes_once_its_text_is_read_is_no_list() {
    let array = json!({"type": "array", "contains": {"type": "integer"}});
    let keys = ["b", "c"];
    let nested = |depth: usize, inner: Value| {
        (0..depth)
            .rev()
            .fold(inner, |inner, level| json!({keys[level]: inner}))
    };
    for combinator in ["oneOf", "anyOf"] {
        for listed in [json!("1.5"), json!(1.5)] {
            for depth in [1, 2] {
                // `b` (depth 1) or `b/c` (depth 2) holds the listed value.
                let option = (0..depth).rev().fold(json!({"enum": [listed]}), |inner, level| {
                    json!({"required": [keys[level]], "properties": {keys[level]: inner}})
                });
                let written = nested(depth, json!(1.5));
                for options in [json!([option, array]), json!([array, option])] {
                    let schema = json!({"properties": {"a": {combinator: options}}});
                    let lone = read(&schema, json!({"a": written}));
                    assert_eq!(
                        lone,
                        json!({"a": nested(depth, listed.clone())}),
                        "{schema}"
                    );
                    let repeated = json!({"a": [written, written]});
                    assert_eq!(read(&schema, repeated.clone()), repeated, "{schema}");
                    #[cfg(feature = "schema-validation")]
                    for reading in [lone, repeated] {
                        let validator = jsonschema::validator_for(&schema).unwrap();
                        assert!(validator.is_valid(&reading), "{schema} {reading}");
                    }
                }
            }
        }
    }
}
