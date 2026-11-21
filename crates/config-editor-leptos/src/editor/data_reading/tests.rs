//! The data reading says how each node reads: a value written once read
//! as a list of one, and a scalar the schema reads otherwise than parsed.

use super::conform::conform;
use super::{Node, Occurs, Scalar};
use crate::editor::language::Repetition;
use serde_json::json;

#[test]
fn a_value_written_once_read_as_a_list_names_its_written_pointer() {
    let schema = json!({"properties": {"dns": {"type": "array", "items": {
        "type": "object", "properties": {"port": {"type": "integer"}}}}}});
    let reading = conform(
        json!({"dns": {"port": "53"}}),
        &schema,
        Repetition::Elements,
        true,
    );
    assert_eq!(reading.value(), &json!({"dns": [{"port": 53}]}));
    assert_eq!(reading.node("/dns").occurs, Occurs::ListOfOne);
    assert_eq!(reading.node("/dns/0").occurs, Occurs::AsWritten);
    assert_eq!(reading.written("/dns/0/port").as_deref(), Some("/dns/port"));
    assert_eq!(reading.written("/dns/0").as_deref(), Some("/dns"));
    assert_eq!(reading.written("/dns"), None);
    assert_eq!(
        reading.node("/dns/0/port"),
        Node {
            occurs: Occurs::AsWritten,
            scalar: Scalar::Retyped(&json!("53")),
        }
    );
    assert_eq!(reading.as_parsed(), Some(json!({"dns": {"port": "53"}})));
}

#[test]
fn a_list_written_by_repeating_is_read_as_written() {
    let schema = json!({"properties": {"dns": {"type": "array", "items": {"type": "string"}}}});
    let reading = conform(
        json!({"dns": ["a", "b"]}),
        &schema,
        Repetition::Elements,
        true,
    );
    assert_eq!(reading.node("/dns").occurs, Occurs::AsWritten);
    assert_eq!(reading.written("/dns/0"), None);
    assert_eq!(reading.as_parsed(), None);
}

#[test]
fn an_hcl_block_written_once_is_a_list_of_one_only_where_the_schema_wants_one() {
    let list = json!({"properties": {"listen": {"type": "array", "items": {"type": "object"}}}});
    let one = json!({"properties": {"listen": {"type": "object"}}});
    let value = json!({"listen": {"port": 80}});
    let read = conform(value.clone(), &list, Repetition::Blocks, false);
    assert_eq!(read.value(), &json!({"listen": [{"port": 80}]}));
    assert_eq!(
        read.written("/listen/0/port").as_deref(),
        Some("/listen/port")
    );
    let read = conform(value.clone(), &one, Repetition::Blocks, false);
    assert_eq!(read.value(), &value);
    assert_eq!(read.written("/listen/port"), None);
}

#[test]
fn a_list_made_while_rereading_an_object_moves_what_is_noted_under_it() {
    // Each option alone allows the lone `<s>` as text, the `oneOf` refuses
    // the object (the second option asks a `t` too): the reread makes `s`
    // the list the first option asks.
    let schema = json!({"properties": {"o": {"oneOf": [
        {"properties": {"s": {"type": "array"}}, "required": ["s"]},
        {"properties": {"s": {"type": "string"}}, "required": ["s", "t"]}
    ]}}});
    let reading = conform(
        json!({"o": {"s": "a"}}),
        &schema,
        Repetition::Elements,
        true,
    );
    assert_eq!(reading.value(), &json!({"o": {"s": ["a"]}}));
    assert_eq!(reading.node("/o").occurs, Occurs::AsWritten);
    assert_eq!(reading.node("/o/s").occurs, Occurs::ListOfOne);
    assert_eq!(reading.written("/o/s/0").as_deref(), Some("/o/s"));
    assert_eq!(reading.as_parsed(), Some(json!({"o": {"s": "a"}})));
}

/// A lone object the schemas refuse, and refuse as a list's item as
/// parsed, is that item where its text read as the item's schema wants
/// it is taken: `<c>true</c>` is the text an item's `c` asks.
#[test]
fn a_lone_object_is_a_list_item_once_its_text_is_read_as_the_item_wants() {
    let schema = json!({"properties": {"x_one": {"oneOf": [
        {"type": "object", "additionalProperties": false},
        {"type": "array", "items": {"properties": {"c": {"type": "string"}}}}]}}});
    let reading = conform(json!({"x_one": {"c": true}}), &schema, Repetition::Elements, true);
    assert_eq!(reading.value(), &json!({"x_one": [{"c": "true"}]}));
    assert_eq!(reading.node("/x_one").occurs, Occurs::ListOfOne);
    assert_eq!(
        reading.node("/x_one/0/c").scalar,
        Scalar::Retyped(&json!(true))
    );
    assert_eq!(reading.as_parsed(), Some(json!({"x_one": {"c": true}})));
    // An object an option takes stays itself.
    let taken = json!({"properties": {"x_one": {"oneOf": [
        {"type": "object"},
        {"type": "array", "items": {"properties": {"c": {"type": "string"}}}}]}}});
    let reading = conform(json!({"x_one": {"c": true}}), &taken, Repetition::Elements, true);
    assert_eq!(reading.value(), &json!({"x_one": {"c": true}}));
}

#[test]
fn text_kept_as_parsed_is_not_noted() {
    let schema = json!({"properties": {"v": {"type": "string"}}});
    let reading = conform(json!({"v": "8"}), &schema, Repetition::Elements, true);
    assert_eq!(reading.node("/v").scalar, Scalar::AsParsed);
    // A number the parser read is the text a string schema wants.
    let reading = conform(json!({"v": 8}), &schema, Repetition::Elements, true);
    assert_eq!(reading.value(), &json!({"v": "8"}));
    assert_eq!(reading.node("/v").scalar, Scalar::Retyped(&json!(8)));
    // An empty element where text is wanted.
    let reading = conform(json!({"v": {}}), &schema, Repetition::Elements, true);
    assert_eq!(reading.value(), &json!({"v": ""}));
    assert_eq!(reading.node("/v").scalar, Scalar::Retyped(&json!({})));
}
