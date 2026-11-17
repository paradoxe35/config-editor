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
