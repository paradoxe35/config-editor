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

#[test]
#[ignore = "FZ38-1"]
fn a_suggested_key_a_dependent_schema_requires_another_for_adds_no_problem() {
    holds_everywhere(
        super::suggestions::check,
        json!({"dependentSchemas": {"name": {"required": ["p"]}}, "properties": {"name": {}}}),
        json!({}),
        json!({"name": []}),
    );
}

#[test]
#[ignore = "FZ38-2"]
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
#[ignore = "FZ38-3"]
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
#[ignore = "FZ38-4"]
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
#[ignore = "FZ38-5"]
fn a_lone_element_under_untyped_list_keywords_reads_as_itself_in_an_xml_view() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"b": {"properties": {"port": {"oneOf": [{"prefixItems": [{}]}]}}}}}),
        json!({"b": {"port": false}}),
        json!({"b": {"port": false}}),
    );
}

#[test]
#[ignore = "FZ38-16"]
fn xml_view_text_the_schema_types_saves_as_the_view_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"a": {"allOf": [{"type": "string"}]}}}),
        json!({"a": true}),
        json!({"a": "true"}),
    );
}

#[test]
#[ignore = "FZ38-6"]
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
#[ignore = "FZ38-7"]
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
