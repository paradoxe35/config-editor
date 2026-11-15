//! The smallest cases of the open findings the random runs found, in every
//! language: ignored until their fix (which removes the `#[ignore]` and the
//! finding's entry in `known.rs`).

use serde_json::{Value, json};

use super::runner::{Case, Verdict};
use crate::editor::Language;

fn holds_everywhere(check: fn(&Case) -> Verdict, schema: Value, doc: Value, other: Value) {
    let mut failed = Vec::new();
    for &language in Language::ALL {
        let case = Case {
            language,
            pick: 0,
            schema: schema.clone(),
            doc: doc.clone(),
            other: other.clone(),
        };
        if let Verdict::Fails(failure) = check(&case) {
            failed.push(format!("{}\n{}", failure.class, failure.detail));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
}

/// FZ38-1: a key whose dependent schema refuses a value written beside
/// it (`size` must be "y" once `name` is there) is not offered; one whose
/// dependency only requires another key is (a documented decision).
#[test]
fn a_key_whose_dependent_schema_refuses_a_value_beside_it_is_not_suggested() {
    holds_everywhere(
        super::suggestions::check,
        json!({"dependentSchemas": {"name": {"properties": {"size": {"const": "y"}}}},
            "properties": {"name": {"type": "integer"}, "size": {"type": "string"}}}),
        json!({"size": "n"}),
        json!({"name": 1, "size": "y"}),
    );
}

#[test]
fn a_problem_on_an_object_holding_a_secret_does_not_quote_it() {
    holds_everywhere(
        super::secrets::check,
        json!({"$defs": {"d0": {"x-secret": true}},
            "patternProperties": {"^x_": {"type": "string", "readOnly": true}},
            "properties": {"x_two": {"oneOf": [{"properties": {"name": {"$ref": "#/$defs/d0"}}}]}}}),
        json!({"x_two": {"name": "sekret1q563"}}),
        Value::Null,
    );
}

#[test]
fn validation_under_recursive_all_of_members_grows_with_the_document() {
    holds_everywhere(
        super::work::check,
        json!({"type": "object", "properties": {"root": {"$ref": "#/$defs/node"}},
        "$defs": {"node": {"allOf": [
            {"type": "object", "properties": {"mode": {"type": "string"}, "c": {"$ref": "#/$defs/node"}}, "required": ["mode"]},
            {"type": "object", "properties": {"d": {"type": "integer"}, "c": {"$ref": "#/$defs/node"}}, "required": ["d"]},
            {"type": "object", "properties": {"e": {"type": "integer"}, "c": {"$ref": "#/$defs/node"}}, "required": ["e"]}
        ]}}}),
        json!({"a": 1}),
        Value::Null,
    );
}

#[test]
fn deprecation_warnings_under_recursive_options_grow_with_the_document() {
    holds_everywhere(
        super::work::check,
        json!({"type": "object", "properties": {"root": {"$ref": "#/$defs/node"}},
        "$defs": {"node": {"oneOf": [
            {"properties": {"x_two": {"type": "number", "deprecated": true},
                "c": {"$ref": "#/$defs/node"}}},
            {"type": "object", "additionalProperties": false,
                "properties": {"name": {"type": "object", "minProperties": 1},
                    "c": {"$ref": "#/$defs/node"}}}
        ]}}}),
        json!({"x_two": 1}),
        Value::Null,
    );
}

#[test]
fn a_lone_element_under_untyped_list_keywords_reads_as_itself_in_an_xml_view() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"b": {"properties": {"port": {"oneOf": [{"prefixItems": [{}]}]}}}}}),
        json!({"b": {"port": false}}),
        json!({"b": {"port": false}}),
    );
}

/// FZ38-16: text typed in an XML view that an applicator types (`allOf`
/// a string over a host's boolean) saves as the view reads it. (Text left
/// as the view showed it saves nothing: an unchanged view keeps its host.)
#[test]
fn xml_view_text_the_schema_types_saves_as_the_view_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"a": {"allOf": [{"type": "string"}]}}}),
        json!({"a": true}),
        json!({"a": "false"}),
    );
}

#[test]
fn xml_text_is_typed_so_that_one_one_of_option_matches() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"size": {"oneOf": [{},
            {"anyOf": [{"properties": {"port": {"type": "string"}}}]}]}}}),
        json!({"size": {"port": 0}}),
        Value::Null,
    );
}

#[test]
fn a_secret_marked_in_an_option_is_not_quoted_in_its_problem() {
    holds_everywhere(
        super::secrets::check,
        json!({"properties": {"b": {"anyOf": [
            {"properties": {"b": {"properties": {"size": {"writeOnly": true, "maxLength": 0}}}}},
            {"type": "string", "readOnly": true}]}}}),
        json!({"b": {"b": {"size": "sekret3q390"}}}),
        Value::Null,
    );
}

#[test]
#[ignore = "FZ38-8"]
fn a_list_emptied_in_a_view_of_an_xml_host_waits_to_save() {
    holds_everywhere(
        super::edit_as::check,
        json!({"$defs": {"d0": {"properties": {"c": {"anyOf": [{"items": {}}]}}}},
            "properties": {"x_two": {"$ref": "#/$defs/d0"}}}),
        json!({"x_two": {"c": false}}),
        json!({"x_two": {"c": []}}),
    );
}

/// FZ38-15: no key is offered in an object holding its `maxProperties`.
#[test]
fn no_key_is_suggested_past_max_properties() {
    holds_everywhere(
        super::suggestions::check,
        json!({"maxProperties": 2, "properties": {"d": {}}}),
        json!({"x_one": {}, "a": 0}),
        json!({"d": 1}),
    );
}

/// FZ38-10/12: a default (or placeholder) the schemas beside it refuse is
/// not written (`allOf` a boolean beside `default: 0`; `exclusiveMinimum`).
#[test]
fn a_refused_default_or_placeholder_is_not_written() {
    for (schema, witness) in [
        (
            json!({"properties": {"c": {"allOf": [{"type": "boolean"}, {"default": 0}]}}}),
            json!({"c": true}),
        ),
        (
            json!({"properties": {"c": {"type": "integer", "exclusiveMinimum": 0}}}),
            json!({"c": 1}),
        ),
    ] {
        holds_everywhere(
            super::fill::check,
            schema.clone(),
            witness.clone(),
            Value::Null,
        );
        holds_everywhere(super::suggestions::check, schema, json!({}), witness);
    }
}

/// FZ38-13: a list whose items have places is not repeated in XML (a new
/// item in front would move the others).
#[test]
fn a_list_with_placed_items_is_not_repeated_in_xml() {
    holds_everywhere(
        super::suggestions::check,
        json!({"properties": {"name": {"prefixItems": [{}, {"enum": [0]}]}}}),
        json!({"name": 0}),
        json!({"name": [1, 0]}),
    );
}

/// FZ38-9/11: a fill writes what an `else` beside a property asks of it
/// too, and the keys a required one makes required; only the values a
/// block writes are Tab stops.
#[test]
fn fills_meet_every_schema_at_a_child_and_stop_only_at_written_values() {
    for (schema, witness) in [
        (
            json!({"properties": {"name": {"allOf": [{
                "else": {"properties": {"a": {"pattern": "^.{2,}$"}}},
                "properties": {"a": {"type": "string", "pattern": "^[a-z]+$"}},
                "required": ["x_one", "a"],
                "if": {"properties": {"x_one": {"const": false}}}}]}}}),
            json!({"name": {"x_one": true, "a": "ab"}}),
        ),
        (
            json!({"properties": {"mode": {"type": "object",
                "properties": {"name": {"enum": [1.5, 2]}, "x_one": {"type": "string"}},
                "required": ["x_one"], "dependentRequired": {"x_one": ["name"]}}}}),
            json!({"mode": {"x_one": "a", "name": 1.5}}),
        ),
        (
            json!({"properties": {"b": {"type": "array", "items": {"type": "object",
                "properties": {"a": {"type": "string"}, "b": {"type": "integer"}},
                "required": ["a", "b"]}}}}),
            json!({"b": [{"a": "x", "b": 1}]}),
        ),
    ] {
        holds_everywhere(super::fill::check, schema, witness, Value::Null);
    }
}

/// FZ38-6 too: XML text in an object under `anyOf` options is read so
/// that the object validates (`-3` the text option two's `c` wants).
#[test]
fn xml_text_is_read_so_that_its_object_validates() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"size": {"anyOf": [
            {"required": ["b"], "properties": {"b": {"allOf": [{"required": ["m"]}]}}},
            {"properties": {"c": {"type": "string"}}}]}}}),
        json!({"size": {"b": {"b": 0}, "c": "-3"}}),
        Value::Null,
    );
}

/// FZ38-7 too: in XML a secret an item's schema marks is masked where the
/// one item is written alone.
#[test]
fn a_secret_of_a_lone_xml_item_is_not_quoted() {
    holds_everywhere(
        super::secrets::check,
        json!({"$defs": {"d0": {"type": "object"}}, "properties": {"b": {"anyOf": [
            {"type": "array", "prefixItems": [{"properties": {"b": {"writeOnly": true}}}]},
            {"properties": {"b": {"$ref": "#/$defs/d0"}}}]}}}),
        json!({"b": [{"b": "sekret1q119"}]}),
        Value::Null,
    );
}
