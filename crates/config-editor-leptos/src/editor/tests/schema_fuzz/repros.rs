//! The smallest cases the random runs found, each checked in every
//! language: a fixed class stays fixed; an open finding's case is ignored
//! until its fix (which removes the `#[ignore]` and the finding's entry in
//! `known.rs`).

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

/// [`holds_everywhere`] for the property `property`, where a language's
/// documented limit (an entry of `known`) may explain a failure.
fn holds_everywhere_but_known(
    property: &str,
    check: fn(&Case) -> Verdict,
    schema: Value,
    doc: Value,
    other: Value,
) {
    let mut failed = Vec::new();
    for &language in Language::ALL {
        let case = Case {
            language,
            pick: 0,
            schema: schema.clone(),
            doc: doc.clone(),
            other: other.clone(),
        };
        if let Verdict::Fails(failure) = check(&case)
            && super::known::excused(property, &failure.class, &case).is_none()
        {
            failed.push(format!("{}\n{}", failure.class, failure.detail));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
}

/// A key whose dependent schema refuses a value written beside
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

/// List keywords with no `type` (`contains`, `uniqueItems`, `items`) say
/// nothing of a value that is no list, however deep the options they are
/// written in: a lone XML element under them reads as its text.
#[test]
fn a_lone_element_under_untyped_list_keywords_reads_as_itself_at_any_option_depth() {
    let list = json!({"contains": {"const": "fast"}, "items": {}});
    let unique =
        json!({"uniqueItems": true, "minContains": 2, "contains": {}, "prefixItems": [{}]});
    let wrapped = |schema: &Value, keyword: &str, depth: usize| {
        let mut schema = json!({"properties": {"b": schema}});
        for _ in 0..depth {
            schema = json!({keyword: [schema]});
        }
        json!({"$defs": {"d": schema}, "properties": {"size": {"$ref": "#/$defs/d"}}})
    };
    for inner in [&list, &unique] {
        for keyword in ["anyOf", "oneOf", "allOf"] {
            for depth in 0..4 {
                holds_everywhere(
                    super::validation::check,
                    wrapped(inner, keyword, depth),
                    json!({"size": {"b": "f"}}),
                    Value::Null,
                );
            }
        }
    }
}

/// Text typed in an XML view that an applicator types (`allOf`
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

/// No key is offered in an object holding its `maxProperties`.
#[test]
fn no_key_is_suggested_past_max_properties() {
    holds_everywhere(
        super::suggestions::check,
        json!({"maxProperties": 2, "properties": {"d": {}}}),
        json!({"x_one": {}, "a": 0}),
        json!({"d": 1}),
    );
}

/// A default (or placeholder) the schemas beside it refuse is
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

/// A list whose items have places is not repeated in XML (a new
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

/// A fill writes what an `else` beside a property asks of it
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

/// XML text in an object under `anyOf` options is read so
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

/// In XML a secret an item's schema marks is masked where the
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

/// P2: a key renamed in an XML view onto one whose schema types its text
/// (`name: "true"` made `size`, a boolean) saves as the view reads it.
#[test]
fn a_key_renamed_onto_a_typed_one_saves_as_the_view_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"size": {"type": "boolean"}}}),
        json!({"name": "true"}),
        json!({"size": true}),
    );
}

/// P1: a value no message may quote under one reading (a secret deep in
/// one `anyOf` option) is not quoted under another (the other option
/// reads its field as a list, so XML reads the element as a list of one).
#[test]
fn a_secret_one_option_marks_is_not_quoted_where_another_reads_a_list() {
    holds_everywhere(
        super::secrets::check,
        json!({"properties": {"b": {"anyOf": [
            {"properties": {"name": {"type": "object",
                "properties": {"name": {"properties": {"name": {"writeOnly": true}}}}}}},
            {"required": ["name"], "properties": {"name": {"type": "array",
                "prefixItems": [{"type": "null"}]}}}]}}}),
        json!({"b": {"name": {"name": {"name": "sekret1q676"}}}}),
        Value::Null,
    );
}

/// A fill writes the keys an `allOf` member requires, beside a member
/// whose options say nothing of them.
#[test]
fn a_fill_writes_the_keys_an_all_of_member_requires() {
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"d": {"allOf": [{"anyOf": [{"properties": {}}]},
            {"required": ["size"]}]}}}),
        json!({"d": {"size": null}}),
        Value::Null,
    );
}

/// P2: a host block the schema reads as a list of one (HCL `x_two { … }`
/// under `type: array`) does not make the save write a list where the
/// view holds an object.
#[test]
fn a_block_read_as_a_list_of_one_does_not_make_an_object_a_list() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"x_two": {"type": "array"}}}),
        json!({"b": false, "x_two": {"name": false}}),
        json!({"b": false, "name": {"b": false}}),
    );
}

/// A fill (or a key's own value) is one every schema at the field takes:
/// a default the `else` the object takes refuses is not written, nor a
/// list where a pattern types the key `null`.
#[test]
fn a_fill_writes_only_what_every_schema_at_the_field_takes() {
    holds_everywhere(
        super::fill::check,
        json!({"else": {"properties": {"c": {"type": "boolean"}}}, "if": {"required": ["p"]},
            "properties": {"c": {"default": 0}}}),
        json!({"c": false}),
        json!(null),
    );
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"mode": {"oneOf": [{"type": "integer"}, {"type": "object",
            "properties": {"x_one": {"prefixItems": [{}]}}, "required": ["x_one"],
            "patternProperties": {"^x_": {"type": "null"}}}]}}}),
        json!({"mode": 0}),
        json!(null),
    );
}

/// A scalar suggested after a key in a language writing only blocks as a
/// list's items (HCL) is the key's own value, never one of its items.
#[test]
fn a_suggested_scalar_is_the_keys_own_value() {
    holds_everywhere(
        super::suggestions::check,
        json!({"properties": {"port": {"anyOf": [{"type": "null"},
            {"type": "array", "prefixItems": [{"type": "boolean"}]}]}}}),
        json!({"port": null}),
        json!({}),
    );
}

/// XML text in a list whose `contains` asks a text is read as that text
/// where the number reading leaves the list refused.
#[test]
fn a_list_item_is_read_as_its_text_where_contains_asks_one() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"c": {"contains": {"type": "string"}}}}),
        json!({"c": [false, "8"]}),
        json!(null),
    );
}

/// A lone XML element is the one item of a list where the value alone is
/// refused and the list taken: an option's bound refusing the number, or
/// an option its keys only seem to pick typing it otherwise.
#[test]
fn a_lone_element_is_one_item_where_only_the_list_is_taken() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"mode": {"anyOf": [{"type": "integer", "multipleOf": 5},
            {"type": "array"}]}}}),
        json!({"mode": [1]}),
        json!(null),
    );
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"d": {"anyOf": [{"properties": {"x_two": {"type": "array"}}},
            {"anyOf": [{"required": ["x_two"], "properties": {"x_two": {"type": "number"}}}]}]}}}),
        json!({"d": {"x_two": [{"c": "n"}]}}),
        json!(null),
    );
}

/// Suggestions at the end of `{root: {c: [{}]}}` in every language.
fn suggest_in_a_list_item(schema: Value) {
    let both = super::reference::compile(&schema).unwrap();
    for &language in Language::ALL {
        let doc = json!({"root": {"c": [{}]}});
        let Some(text) = both.written(language, &doc) else {
            continue;
        };
        let caret = text.encode_utf16().count();
        drop(both.editor.suggestions(language, &text, caret));
    }
}

/// A key under a list item whose object is written inline (TOML beneath
/// `[[root.c]]`) is suggested without a crash where its schema has a
/// `const` no value meets: one its `not` refuses, beside `anyOf` options
/// describing an object, or one its `type` refuses (P5).
#[test]
fn a_key_with_a_const_its_schema_refuses_is_suggested_inside_a_list_item() {
    let in_item = |port: Value| {
        json!({"properties": {"root": {"properties": {"c": {"items":
        {"properties": {"port": port}}}}}}})
    };
    suggest_in_a_list_item(in_item(json!({"anyOf": [{"const": true},
        {"if": {}, "else": {"properties": {}}}], "not": {"type": "boolean"}})));
    suggest_in_a_list_item(in_item(json!({"const": true, "not": {"type": "boolean"},
        "properties": {}})));
    suggest_in_a_list_item(in_item(json!({"type": "object", "const": 1})));
}

/// The same with a `const` the schema takes but TOML cannot write (an
/// object holding `null`) (P5).
#[test]
fn a_key_with_a_const_toml_cannot_write_is_suggested_inside_a_list_item() {
    suggest_in_a_list_item(json!({"properties": {"root": {"properties": {"c": {"items":
        {"properties": {"port": {"type": "object", "const": {"a": null}}}}}}}}}));
}

/// A fill writes an object or a list only where the schemas at the key
/// take one: not where an `allOf` member types the value a string, a
/// number or a boolean, nor where no list meets them (`oneOf` options a
/// list meets twice, items refused while items are asked).
#[test]
fn a_fill_writes_no_object_or_list_the_schemas_at_the_key_refuse() {
    for (schema, doc) in [
        (
            json!({"properties": {"d": {"allOf": [{"properties": {"x_one": {"items": {}}},
                "required": ["x_one"]}, {"type": "string"}]}}}),
            json!({"d": "a"}),
        ),
        (
            json!({"properties": {"port": {"allOf": [{"type": "boolean"},
                {"properties": {"b": {"prefixItems": [{}]}}, "required": ["b"]}]}}}),
            json!({"port": false}),
        ),
        (
            json!({"properties": {"c": {"allOf": [{"type": "boolean"},
                {"oneOf": [{"type": "array"}, {}]}]}}}),
            json!({"c": false}),
        ),
        (
            json!({"$defs": {"d0": {"anyOf": [{"oneOf": [{}, {"type": "array"}]}]}},
                "properties": {"a": {"$ref": "#/$defs/d0"}}}),
            json!({"a": 0}),
        ),
        (
            json!({"properties": {"x_one": {"oneOf": [{}, {"type": "array"}]}}}),
            json!({"x_one": "f"}),
        ),
        (
            json!({"properties": {"port": {"minItems": 1, "items": false}}}),
            json!({"port": {}}),
        ),
    ] {
        holds_everywhere_but_known("fill", super::fill::check, schema, doc, json!(null));
    }
}

/// A required child is filled as the schemas its object then takes say:
/// the `else` of an `if` the filled object does not meet, the dependent
/// schema of a sibling the fill writes, the schema of a list's first item.
#[test]
fn a_required_child_is_filled_as_its_filled_object_takes_it() {
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"$defs": {"d0": {"required": ["c", "name"], "properties": {"c": {"type": "string"},
            "name": {"if": {"required": ["x"]}, "then": {"properties": {"c": {"const": "s"}}},
                "else": {"properties": {"c": {"type": "null"}}}, "required": ["c"]}}}},
            "properties": {"b": {"$ref": "#/$defs/d0"}}}),
        json!({"b": "t"}),
        json!(null),
    );
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"properties": {"name": {"allOf": [{"properties": {"size": {"type": "array"}},
            "required": ["x_two", "size"],
            "dependentSchemas": {"x_two": {"properties": {"size": {"type": "string"}}}}}]}}}),
        json!({"name": null}),
        json!(null),
    );
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"properties": {"x_two": {"items": {"oneOf": [{"required": ["c"],
            "properties": {"c": {"items": {}}}}]}, "prefixItems": [{"type": "array"}]}}}),
        json!({"x_two": []}),
        json!(null),
    );
}

/// A required child is written where only an `if` the fill does not
/// decide types it otherwise (its `else` saying nothing takes any value),
/// and an object its `minProperties` asks keys of is left to type empty.
#[test]
fn a_required_child_beside_an_undecided_if_or_asking_keys_is_written() {
    for (schema, doc) in [
        (
            json!({"properties": {"mode": {"required": ["port"], "properties": {"port": {
                "if": {"properties": {"size": {"const": 0}}},
                "then": {"properties": {"x_two": {"type": "boolean"}}},
                "properties": {"size": {"type": "string"}, "x_two": {"type": "string"}},
                "required": ["size", "x_two"]}}}}}),
            json!({"mode": {"port": {"size": "f", "x_two": "a"}}}),
        ),
        (
            json!({"$defs": {"d0": {"properties": {"a": {"enum": ["1.5"]}, "mode": {"type": "array"}},
                "required": ["a"], "dependentSchemas": {"a": {"required": ["mode"]}},
                "if": {"properties": {"a": {"const": 0}}},
                "then": {"properties": {"mode": {"type": "string"}}}}},
                "properties": {"d": {"$ref": "#/$defs/d0"}}}),
            json!({"d": {"a": "1.5", "mode": []}}),
        ),
        (
            json!({"properties": {"x_two": {"required": ["a", "mode"], "properties": {
                "a": {"type": "string"}, "mode": {"type": "object", "minProperties": 1}}}}}),
            json!({"x_two": {"a": "x", "mode": {"size": 0}}}),
        ),
    ] {
        holds_everywhere_but_known("fill", super::fill::check, schema, doc, json!(null));
    }
}

/// A key whose dependent schema types a value written beside it otherwise
/// than its own schema does is not suggested, also where XML text reads
/// as either type.
#[test]
fn a_key_whose_dependent_schema_retypes_a_value_beside_it_is_not_suggested() {
    holds_everywhere(
        super::suggestions::check,
        json!({"dependentSchemas": {"c": {"properties": {"b": {"type": "integer"}}}},
            "properties": {"c": {}, "b": {"type": "string"}}}),
        json!({"b": "-3"}),
        json!({"c": 0}),
    );
    holds_everywhere(
        super::suggestions::check,
        json!({"$defs": {"d0": {"type": "string"}}, "properties": {"d": {}, "c": {"$ref": "#/$defs/d0"}},
            "dependentSchemas": {"d": {"properties": {"c": {"type": "integer"}}}}}),
        json!({"c": "-3"}),
        json!({"d": []}),
    );
}

/// Another element of a list written by repeating its key is suggested
/// only where the list then holds: not one its `uniqueItems` refuses.
#[test]
fn another_list_element_is_suggested_only_where_the_list_takes_it() {
    holds_everywhere(
        super::suggestions::check,
        json!({"properties": {"a": {"uniqueItems": true, "items": {"default": 1}}}}),
        json!({"a": 1}),
        json!({"a": []}),
    );
}

/// XML text is read so that its object validates where a reading does:
/// a lone element as a one-item list an option asks, text an option
/// nested in another types.
#[test]
fn xml_text_is_read_so_that_options_holding_it_validate() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"d": {"oneOf": [{"properties": {"name": {"type": "array"}}},
            {"required": ["x"], "properties": {"name": {}}}]}}}),
        json!({"d": {"name": ["n"]}}),
        json!(null),
    );
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"b": {"properties": {"x_two": {"anyOf": [
            {"properties": {"a": {"anyOf": [{"properties": {"name": {"type": "string"}}},
                {"required": ["m"], "properties": {"name": {}}}]}}},
            {"required": ["n"], "properties": {"a": {"minProperties": 1,
                "properties": {"size": {"type": "integer"}}}}}]}}}}}),
        json!({"b": {"x_two": {"a": {"name": "1"}}}}),
        json!(null),
    );
}

/// A lone XML element an option reads as a list's one item (beside an
/// option a list meets) reads so whatever its siblings hold: a key
/// suggested beside it, whose empty value is refused, does not change it.
#[test]
fn a_lone_element_an_option_takes_as_a_list_reads_so_beside_any_sibling() {
    holds_everywhere(
        super::suggestions::check,
        json!({"properties": {"a": {"type": "array", "items": {"type": "string", "minLength": 1}},
            "size": {"oneOf": [{"required": ["x"]}, {"type": "array", "items": {"const": 0}}]}}}),
        json!({"size": {}}),
        json!({"a": []}),
    );
    holds_everywhere(
        super::suggestions::check,
        json!({"properties": {"a": {"type": "array", "items": {"type": "string", "minLength": 1}},
            "size": {"oneOf": [{"properties": {"a": {"enum": [1]}}},
                {"type": "array", "items": {"const": 0}}, {"enum": []}]}}}),
        json!({"size": {"a": 0}}),
        json!({"a": []}),
    );
}

/// A value typed in an XML view saves as the view reads it where the
/// options above it take only that reading, or the schema lists the
/// view's reading.
#[test]
fn xml_view_text_saves_as_read_where_the_schema_takes_only_that_reading() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"b": {"oneOf": [{}, {"properties": {"b": {"type": "string"}}}]}}}),
        json!({"b": {"b": "x"}}),
        json!({"b": {"b": 0}}),
    );
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"name": {"oneOf": [{"properties": {"x_two": {"properties":
            {"c": {"type": "string"}}}}}, {}]}}}),
        json!({"name": {"x_two": {"c": "a"}}}),
        json!({"name": {"x_two": {"c": 0}}}),
    );
    holds_everywhere(
        super::edit_as::check,
        json!({"$defs": {"d0": {"anyOf": [{"type": "object", "properties": {"next": {"$ref": "#/$defs/d0"}}}]}},
            "properties": {"b": {"oneOf": [{"properties": {"name": {}, "a": {"$ref": "#/$defs/d0"}}},
                {"properties": {"name": {"type": "boolean"}}}]}}}),
        json!({"b": {"name": "1"}}),
        json!({"b": {"name": false, "a": {"next": "a"}}}),
    );
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"x_two": {"properties": {"a": {"anyOf": [{"enum": ["true"]}, {}]}}}}}),
        json!({"x_two": {"a": false}}),
        json!({"x_two": {"a": "true"}}),
    );
}
