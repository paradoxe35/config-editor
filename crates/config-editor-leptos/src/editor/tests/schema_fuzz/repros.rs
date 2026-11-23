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
    holds_but_known(property, check, (schema, doc, other), 1);
}

/// [`holds_everywhere_but_known`] for each of the first `picks` second
/// languages a check picks (an Edit-as view of an XML host in each other
/// language).
fn holds_but_known(
    property: &str,
    check: fn(&Case) -> Verdict,
    (schema, doc, other): (Value, Value, Value),
    picks: usize,
) {
    let mut failed = Vec::new();
    for &language in Language::ALL {
        for pick in 0..picks {
            let case = Case {
                language,
                pick,
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

/// P2: a list emptied in a view of an XML host (`c: []` where the host
/// has `<c>false</c>`) waits to save, as XML writes no empty list.
#[test]
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

/// A lone XML element is the one item of a list where the scalar reading
/// is refused (by another option's `enum`) and the list's item keeps its
/// text where the item asks a string.
#[test]
fn a_lone_element_is_one_text_item_where_the_scalar_reading_is_refused() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"x_one": {"oneOf": [
            {"type": "array", "contains": {"type": "string"}}, {"enum": [0]}]}}}),
        json!({"x_one": ["-3"]}),
        json!(null),
    );
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"c": {"anyOf": [
            {"required": ["d"], "properties": {"size": {}}},
            {"properties": {"size": {"type": "array"}}}]}}}),
        json!({"c": {"size": ["0"]}}),
        json!(null),
    );
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"x_one": {"oneOf": [
            {"properties": {"x_one": {"properties": {"b": {"type": "string"}}}}}, {}]}}}),
        json!({"x_one": {"x_one": {"b": 0}}}),
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

/// A key the object requires whichever option it takes is filled: one
/// beside a choice of several options (which stays the user's to pick),
/// and one every option of a `oneOf`/`anyOf` requires (a choice of one
/// option, or of options asking the same key).
#[test]
fn a_key_every_option_requires_is_filled() {
    for schema in [
        json!({"properties":{"mode":{"allOf":[{"required":["s"]},{"anyOf":[{"required":["x"]},{"required":["x_two"]}]}]}}}),
        json!({"properties":{"mode":{"required":["s"],"properties":{"s":{"type":"string"}},"oneOf":[{"required":["x"]},{"required":["y"]}]}}}),
        json!({"properties":{"mode":{"allOf":[{"required":["s"]},{"anyOf":[{"required":["x"]},{"required":["x"]}]}]}}}),
        json!({"properties":{"mode":{"type":"object","properties":{"p":{"type":"string"}},"oneOf":[{"required":["p"]}]}}}),
        json!({"properties":{"mode":{"anyOf":[{"required":["x","a"]},{"allOf":[{"required":["x"]}],"required":["b"]}]}}}),
    ] {
        holds_everywhere_but_known(
            "fill",
            super::fill::check,
            schema,
            json!({"mode": "x"}),
            json!(null),
        );
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

/// P2: a lone XML element whose text an object option lists (`b:
/// enum ["1.5"]`) beside an option for a list reads as that object, not
/// a list of one, so the save writes what the view reads.
#[test]
fn a_lone_element_an_object_option_lists_saves_as_that_object() {
    holds_everywhere(
        super::edit_as::check,
        json!({"$defs": {"d0": {"required": ["b"], "properties": {"b": {"enum": ["1.5"]}}}},
            "properties": {"a": {"oneOf": [{"$ref": "#/$defs/d0"},
                {"type": "array", "contains": {"type": "integer"}}]}}}),
        json!({"a": {"x_two": "1.5"}}),
        json!({"a": {"b": "1.5"}}),
    );
}

/// A number typed in an XML view where an overlapping `oneOf` (`{}`
/// beside a string option) would take the host's string twice saves as
/// the view reads it (P2, PutGet).
#[test]
fn a_number_under_an_overlapping_one_of_saves_as_the_view_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"x_one": {"oneOf": [{}, {"properties": {"x_one": {"type": "string"}}}]}}}),
        json!({"x_one": {"x_one": "-"}}),
        json!({"x_one": {"x_one": 0}}),
    );
}

/// P2: a number typed in an XML view where options nearer the value
/// (`anyOf` in the items) take the host's string, but an outer `oneOf`
/// with an anything option takes only the view's reading, saves as the
/// view reads it: the outermost schema
/// telling the two readings apart decides.
#[test]
fn a_number_under_nested_options_saves_as_the_outer_one_of_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"c": {"oneOf": [
            {"items": {"anyOf": [{"properties": {"mode": {"type": "string"}}}]}}, {}]}}}),
        json!({"c": [{"mode": "n"}, false]}),
        json!({"c": [{"mode": "0"}, {"a": "Z"}]}),
    );
}

/// P2: text typed in an XML view as a list's item, where only the list's
/// `contains` decides its reading (`true` taken, the host's string type
/// `"true"` refused), saves as the view reads it.
#[test]
fn an_item_only_contains_decides_saves_as_the_view_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"d": {"type": "array", "contains": {"enum": [true]}}}}),
        json!({"d": ["a"]}),
        json!({"d": true}),
    );
}

/// P2: a host's string respelled as a number in an XML view (`"8"` typed
/// `08`) where options above take only the number saves as the view reads
/// it: the view showed the string as 8, and the edit wrote it anew.
#[test]
fn a_string_respelled_as_a_number_the_options_take_saves_as_the_view_reads_it() {
    holds_everywhere(
        super::edit_as::check,
        json!({"properties": {"x_one": {"properties": {"d": {"oneOf": [
            {"properties": {"c": {"type": "integer"}}}]}}}}}),
        json!({"x_one": {"d": {"c": "8"}}}),
        json!({"x_one": {"d": {"c": "08"}}}),
    );
}

/// P2: text typed over a host's boolean where the schema takes only text
/// (`"true"` over `true`, `d` a string) saves as the view reads it, in
/// every view and host, spelled anew or not (`True`).
#[test]
fn text_typed_over_a_boolean_the_schema_types_text_saves_as_the_view_reads_it() {
    for typed in ["true", "True"] {
        holds_everywhere(
            super::edit_as::check,
            json!({"properties": {"d": {"type": "string"}}}),
            json!({"d": true}),
            json!({ "d": typed }),
        );
    }
}

/// A required child under a `oneOf` with an option taking anything is
/// not filled with the first option's required `b`, which the anything
/// option takes too: no option's fill is taken, so the slot is left to
/// type.
#[test]
fn a_fill_under_a_one_of_with_an_anything_option_writes_no_value_two_options_take() {
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"name": {"required": ["x_one", "mode"], "properties": {"mode": {"oneOf": [
            {"required": ["b"], "properties": {"b": {"type": "boolean"}}}, {},
            {"dependentSchemas": {"x_one": {"required": ["s"]}}}]}}}}}),
        json!({"name": {"mode": {"x_one": null}, "x_one": false}}),
        Value::Null,
    );
}

/// An accepted key's value is a list only where every schema at the key
/// takes one: a pattern typing `x_` keys text beside the property's list
/// keywords makes it text to type, not `[]`.
#[test]
fn an_accepted_key_is_written_as_every_schema_at_it_takes() {
    holds_everywhere(
        super::fill::check,
        json!({"$defs": {"d0": {"anyOf": [{"items": {}}]}}, "patternProperties": {"^x_": {"type": "string"}},
            "properties": {"x_one": {"$ref": "#/$defs/d0"}}}),
        json!({"x_one": "1"}),
        Value::Null,
    );
}

/// A fill leaves out the values a language cannot write (TOML a `null`
/// item, XML an empty list a required child is) and is otherwise JSON's:
/// only that documented limit makes the schema refuse it.
#[test]
fn a_fill_leaves_out_only_what_its_language_cannot_write() {
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"port": {"properties": {"size": {"minItems": 1, "items": {"type": "null"}}},
            "required": ["size"]}}}),
        json!({"port": {"size": null}}),
        Value::Null,
    );
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"b": {"properties": {"mode": {"anyOf": [{"dependentSchemas":
            {"name": {"required": ["p"]}}, "properties": {"name": {"type": "array"}}, "required": ["name"]}]}},
            "required": ["port", "mode"]}}}),
        json!({"b": {"port": {}, "mode": 0}}),
        Value::Null,
    );
}

/// A key whose `if` turns on a `then` refusing a value written beside it
/// (`size` a string once `b` is written) is not suggested, as a key whose
/// dependent schema does.
#[test]
fn a_key_whose_if_refuses_a_value_beside_it_is_not_suggested() {
    holds_everywhere(
        super::suggestions::check,
        json!({"if": {"required": ["b"]}, "then": {"properties": {"size": {"type": "string"}}},
            "properties": {"b": {}, "size": {"type": "integer"}}}),
        json!({"size": 0}),
        json!({"b": "8"}),
    );
}

/// A required child object an only `oneOf` option describes is filled the
/// same nested inside a required object (where the caret went to an
/// earlier value, so its first key is written whole) as at the root: met
/// where some object meets it; else its field no value meets (`c` a
/// number by a required key's dependent schema and a string by
/// `additionalProperties`) is the documented class, found through the
/// dependent schema.
#[test]
fn a_fill_nested_under_a_one_of_option_is_met_or_has_a_field_no_value_meets() {
    let option = |properties: Value| {
        json!({"oneOf": [{"dependentSchemas": {"d": {"properties": {"c": {"type": "number"}}}},
            "additionalProperties": {"type": "string", "pattern": "^[a-z]+$"},
            "properties": properties, "required": ["d", "c"]}]})
    };
    let nested = |size: Value| {
        json!({"properties": {"size": {"properties": {"b": {"enum": ["s"]}, "size": size},
            "required": ["b", "size"]}}})
    };
    // `c` is named a number: met.
    holds_everywhere(
        super::fill::check,
        nested(option(json!({"c": {"type": "number"}}))),
        json!({"size": []}),
        json!(null),
    );
    // `c` is met by no value.
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        nested(option(json!({}))),
        json!({"size": []}),
        json!(null),
    );
}

/// A lone XML element an inner `oneOf` refuses as an object (no keys
/// allowed) is the one item of the list the other option takes: the valid
/// reading, with no problem.
#[test]
fn a_lone_element_an_inner_option_refuses_is_the_item_of_the_list_the_other_takes() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"x_one": {"oneOf": [
            {"oneOf": [{"type": "object", "additionalProperties": false}]},
            {"type": "array", "items": {"x_one": {}}}]}}}),
        json!({"x_one": [{"c": "t"}]}),
        json!(null),
    );
    // The same, its item's text read as the item's schema wants it.
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"x_one": {"oneOf": [
            {"oneOf": [{"type": "object", "additionalProperties": false}]},
            {"type": "array", "items": {"properties": {"c": {"type": "string"}}}}]}}}),
        json!({"x_one": [{"c": "true"}]}),
        json!(null),
    );
}

/// A number respelled in an XML view (`08` made `8`) where the host holds
/// the text `"08"` and the view reads it as a list's one item the schema
/// takes (and not the host's text) saves as that list (P2): the edit is
/// no spelling of the host's own value.
#[test]
fn a_number_respelled_where_the_view_reads_a_host_scalar_as_a_list_saves_the_list() {
    holds_everywhere(
        super::edit_as::check,
        json!({"allOf": [{"properties": {"x_one": {"type": "array",
            "prefixItems": [{"type": "number"}]}}}]}),
        json!({"x_one": "08"}),
        json!({"x_one": 8}),
    );
}

/// A required child list asking more items (`minItems: 2`) than its item
/// schemas say anything of (one `prefixItems` entry, no `items`) is
/// written with all of them, the free one any value the language writes.
#[test]
fn a_required_child_list_with_an_item_no_schema_types_is_written_with_it() {
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"properties": {"x_two": {"required": ["c"], "properties": {"c": {
            "minItems": 2, "prefixItems": [{"minimum": 0}]}}}}}),
        json!({"x_two": {"c": 0}}),
        json!(null),
    );
}

/// A required child list asking an item (`minItems: 1`) of text a
/// pattern spells (`^x`, through a reference), which no plain letters
/// meet, is written with one such item, not empty.
#[test]
fn a_required_child_list_of_patterned_text_is_written_with_its_items() {
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"$defs": {"d1": {"type": "string", "pattern": "^x"}},
            "properties": {"c": {"required": ["port"], "properties": {"port": {
                "minItems": 1, "items": {"$ref": "#/$defs/d1"}}}}}}),
        json!({"c": {"port": "x"}}),
        json!(null),
    );
}

/// A required child whose property says nothing of it (a description)
/// while an `allOf` member beside it asks a list of two is written as
/// that list in every language (XML wrote one empty element).
#[test]
fn a_required_child_list_asked_beside_its_property_is_written_with_its_items() {
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"properties": {"x_one": {"allOf": [{"properties": {"x_two": {
            "type": "array", "minItems": 2}}}],
            "properties": {"x_two": {"description": "d"}}, "required": ["x_two"]}}}),
        json!({"x_one": 0}),
        json!(null),
    );
}

/// A key renamed in an XML view keeps what is under it as the host held
/// it, each value told by its own documented XML spelling: the host's
/// text `"0"` the view reads as `0`, and a list of one emptied text the
/// view shows as an empty element.
#[test]
fn values_under_a_key_renamed_in_an_xml_view_keep_the_documented_host_forms() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({}),
        json!({"a": {"port": "0"}}),
        json!({"size": {"port": 0}}),
    );
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({}),
        json!({"a": [""]}),
        json!({"port": ""}),
    );
    // A key renamed inside a list of one the view shows as its item.
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({}),
        json!({"port": [{"c": ["t"]}]}),
        json!({"port": {"size": 1}}),
    );
}

/// A required child list asking an item no schema lets be (`items:
/// false`) is written empty: the documented least list.
#[test]
fn a_required_child_list_whose_items_are_refused_is_the_documented_empty_list() {
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"$defs": {"d0": {"allOf": [{"properties": {"name": {"minItems": 1, "items": false}}}],
            "properties": {"name": {"type": "array"}}, "required": ["name"]}},
            "properties": {"x_two": {"$ref": "#/$defs/d0"}}}),
        json!({"x_two": 0}),
        json!(null),
    );
    // Its `contains` asks an item `items: false` refuses.
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"$defs": {"d0": {"allOf": [{"properties": {"name": {"minItems": 1,
            "contains": {"type": "integer"}, "items": false}}}],
            "properties": {"name": {"type": "array"}}, "required": ["name"]}},
            "properties": {"x_two": {"$ref": "#/$defs/d0"}}}),
        json!({"x_two": 0}),
        json!(null),
    );
    // Its `contains` no value meets (`enum: []`), in an `allOf` member.
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"properties": {"name": {"allOf": [{"properties": {"b": {
            "contains": {"enum": []}, "items": {}}}}], "required": ["b"]}}}),
        json!({"name": "t"}),
        json!(null),
    );
}

/// A required child list whose `minItems` and `contains` an `allOf`
/// member gives beside the property's `type: array` is written with the
/// items they ask, in every language.
#[test]
fn a_required_child_list_asked_items_by_an_all_of_member_is_written_with_them() {
    holds_everywhere(
        super::fill::check,
        json!({"$defs": {"d0": {"allOf": [{"properties": {"name": {"minItems": 1,
            "contains": {"type": "integer"}}}}],
            "properties": {"name": {"type": "array"}}, "required": ["name"]}},
            "properties": {"x_two": {"$ref": "#/$defs/d0"}}}),
        json!({"x_two": 0}),
        json!(null),
    );
    // A first item only `null` meets (`[null, 0]`, which TOML cannot
    // write: a documented limit).
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"$defs": {"d0": {"allOf": [{"properties": {"name": {"minItems": 1,
            "prefixItems": [{"type": "null"}], "contains": {"type": "integer"}}}}],
            "required": ["name"]}},
            "properties": {"x_two": {"$ref": "#/$defs/d0"}}}),
        json!({"x_two": 0}),
        json!(null),
    );
    // Its `contains` item after the `prefixItems` item that refuses it
    // (`[[], 0]`, which XML cannot write: a documented limit).
    holds_everywhere_but_known(
        "fill",
        super::fill::check,
        json!({"$defs": {"d0": {"allOf": [{"properties": {"name": {"minItems": 1,
            "prefixItems": [{"type": "array"}], "contains": {"type": "integer"}}}}],
            "properties": {"name": {"type": "array"}}, "required": ["name"]}},
            "properties": {"x_two": {"$ref": "#/$defs/d0"}}}),
        json!({"x_two": 0}),
        json!(null),
    );
}

/// A key whose dependent schema refuses a lone XML element read as the
/// list's one item beside it (`port` a list, then also a string) is not
/// suggested.
#[test]
fn a_key_whose_dependent_schema_refuses_a_lone_element_read_as_a_list_is_not_suggested() {
    holds_everywhere(
        super::suggestions::check,
        json!({"dependentSchemas": {"size": {"properties": {"port": {"type": "string"}}}},
            "properties": {"size": {}, "port": {"type": "array"}}}),
        json!({"port": 0}),
        json!({"size": ""}),
    );
}

/// A fill decides an `if` by the siblings it writes: `mode` written as
/// text never meets `mode: {const: false}`, so the `else` applies and
/// `a` is written as it allows, not `0`.
#[test]
fn a_fill_decides_an_if_by_the_siblings_it_writes() {
    holds_everywhere(
        super::fill::check,
        json!({"$defs": {"d0": {"allOf": [{"else": {"properties": {"a": {"exclusiveMinimum": 0}}},
            "properties": {"mode": {"type": "string"}}, "required": ["mode", "a"],
            "then": {"properties": {"a": {"enum": [0]}}},
            "if": {"properties": {"mode": {"const": false}}}}]}},
            "properties": {"c": {"$ref": "#/$defs/d0"}}}),
        json!({"c": false}),
        json!(null),
    );
}

/// An `if` on a sibling's value (its `const`, `enum` or `type`) is
/// decided by the value the fill writes for that sibling, met or missed,
/// and the other sibling is filled by the branch taken, in every
/// language: `then` and `else` ask values no one value meets both of.
#[test]
fn a_fill_reads_an_if_on_a_sibling_by_the_value_it_writes() {
    let cases = [
        // A value left to type, read empty or as its placeholder, misses
        // the `if` either way: `else`.
        (json!({"type": "string"}), json!({"const": false})),
        (json!({"type": "string"}), json!({"const": "x"})),
        (json!({"type": "string"}), json!({"enum": ["x", "y"]})),
        (json!({"type": "string"}), json!({"type": "integer"})),
        // Its default written meets the `if` (`then`), or misses it.
        (
            json!({"type": "string", "default": "on"}),
            json!({"const": "on"}),
        ),
        (
            json!({"type": "string", "default": "on"}),
            json!({"enum": ["on", "x"]}),
        ),
        (
            json!({"type": "integer", "default": 2}),
            json!({"type": "integer"}),
        ),
        (
            json!({"type": "string", "default": "off"}),
            json!({"const": "on"}),
        ),
        (
            json!({"type": "integer", "default": 2}),
            json!({"enum": [1, 3]}),
        ),
    ];
    for (mode, test) in cases {
        holds_everywhere(
            super::fill::check,
            json!({"properties": {"c": {"required": ["mode", "a"],
                "properties": {"mode": mode, "a": {"type": "integer"}},
                "if": {"properties": {"mode": test}},
                "then": {"properties": {"a": {"enum": [5]}}},
                "else": {"properties": {"a": {"maximum": -1}}}}}}),
            json!({"c": false}),
            json!(null),
        );
    }
}

/// A lone XML element an option names a list for, where the list would
/// make its object match both options of a `oneOf`, reads as the value
/// written: the document is valid as written.
#[test]
fn a_lone_element_a_one_of_option_names_a_list_for_reads_as_written_where_the_list_matches_both() {
    holds_everywhere(
        super::validation::check,
        json!({"properties": {"x_two": {"oneOf": [{"properties": {"a": {"type": "array"}}}, {}]}}}),
        json!({"x_two": {"a": "1"}}),
        json!(null),
    );
}

/// A required list a fill writes with its one item reads back as that
/// list in XML, though an option silent on it leaves it to one typing it
/// a string (which the list's own schema contradicts).
#[test]
fn a_filled_list_of_one_reads_as_the_list_beside_an_option_typing_it_otherwise() {
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"b": {"required": ["mode"], "properties": {"mode": {"allOf": [
            {"anyOf": [{"additionalProperties": {"type": "string"}}, {}]},
            {"required": ["size"], "properties": {"size": {"type": "array", "minItems": 1}}}]}}}}}),
        json!({"b": {"mode": {"size": [0]}}}),
        json!(null),
    );
}

/// The host's text `"null"` under a key an XML view renamed, where the
/// view reads `null`, is kept as the host's string (the documented kept
/// text, as for `"8"` and `"true"`).
#[test]
fn a_hosts_null_text_under_a_renamed_key_is_the_documented_kept_text() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({}),
        json!({"a": "null"}),
        json!({"d": "null"}),
    );
}

/// A save whose only difference from the view is a documented XML
/// spelling (a list cut to one element) is that class, though the view
/// also holds a value XML writes only by its schema (`null` under
/// `type: null`), which reads back as the view holds it.
#[test]
fn a_documented_spelling_names_the_save_beside_a_null_the_schema_reads_back() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"properties": {"name": {"anyOf": [{"type": "object"}, {"type": "array"}]},
            "mode": {"type": "null"}}}),
        json!({"name": "f"}),
        json!({"name": [{"size": false}], "mode": null}),
    );
}

/// A key a dependent schema requires is written only beside the key it
/// depends on: `size` (which the `else` the object then takes types
/// otherwise) is not filled where `x_two` is not written.
#[test]
fn a_key_a_dependent_schema_requires_is_filled_only_beside_its_key() {
    let schema = json!({"properties": {"mode": {"required": ["b"],
        "properties": {"b": {"type": "string"},
            "size": {"anyOf": [{"required": ["a"], "properties": {"a": {"type": "number"}}}]}},
        "dependentSchemas": {"x_two": {"required": ["size"]}},
        "if": {"required": ["x"]}, "else": {"properties": {"size": {"type": "integer"}}}}}});
    holds_everywhere(
        super::suggestions::check,
        schema.clone(),
        json!({}),
        json!({"mode": {"b": "a"}}),
    );
    holds_everywhere(
        super::fill::check,
        schema,
        json!({"mode": {"b": "a"}}),
        Value::Null,
    );
}

/// A required child is filled as an object only where every schema at it
/// takes one: not where the `else` its object takes (its sibling `a` left
/// to type misses the `if`) types it a boolean, beside an option
/// describing an object.
#[test]
fn a_required_child_is_filled_as_an_object_only_where_every_schema_at_it_takes_one() {
    holds_everywhere(
        super::fill::check,
        json!({"properties": {"a": {"required": ["a", "c"],
            "properties": {"c": {"oneOf": [{"required": ["name"],
                "properties": {"name": {"type": "number"}}}]}},
            "if": {"properties": {"a": {"const": false}}},
            "else": {"properties": {"c": {"type": "boolean"}}}}}}),
        json!({"a": "t"}),
        Value::Null,
    );
}

/// A reading the schema takes is saved where the document fails elsewhere
/// whichever is saved: `mode` makes `port` a number, and `d`'s `0` is
/// what the `oneOf` takes (its host string `"a"` is not).
#[test]
fn a_reading_the_schema_takes_is_saved_beside_a_failure_elsewhere() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"dependentSchemas": {"mode": {"properties": {"port": {"type": "number"}}}},
            "properties": {"port": {"oneOf": [{"properties": {"d": {"type": "integer"}}},
                {"required": ["n"], "properties": {"d": {}}}]}}}),
        json!({"port": {"d": "a"}}),
        json!({"mode": 0, "port": {"d": 0}}),
    );
}

/// A host string typed in another spelling the schema reads alike (`8`
/// over `"08"`, both a list's number) saves as typed.
#[test]
fn a_string_respelled_where_the_schema_reads_both_alike_saves_as_typed() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"allOf": [{"properties": {"size": {"type": "array", "items": {"enum": [0]}}}}]}),
        json!({"size": "08"}),
        json!({"size": "8"}),
    );
}

/// A lone XML element an option reads as a list (`b` under `type:
/// array`) is that list to the view's problems as to its host's: two
/// options taking it are a problem in both.
#[test]
fn a_lone_element_an_option_reads_as_a_list_is_judged_alike_in_view_and_host() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"properties": {"d": {"oneOf": [
            {"required": ["size"], "properties": {"b": {"type": "array"}}},
            {"x_two": {}}]}}}),
        json!({"d": {"b": [false]}}),
        json!({"d": {"b": false, "size": "x"}}),
    );
}

/// A host scalar written as two elements (`0` typed twice in an XML view)
/// is a list the host never had: under a schema taking only strings as
/// its items, each saves as the view reads it.
#[test]
fn a_host_scalar_typed_as_two_elements_saves_as_the_schema_reads_them() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"$defs": {"d0": {"anyOf": [{"items": {"type": "string"}}]}},
            "properties": {"d": {"$ref": "#/$defs/d0"}}}),
        json!({"d": 0}),
        json!({"d": [0, "0"]}),
    );
}

/// A host string typed over in an XML view where the schema reads the
/// element as a list of one list of one item (`<d>null</d>` read as
/// `[[null]]`) saves as the view reads it: the user wrote it.
#[test]
fn a_string_typed_over_where_the_schema_reads_nested_lists_of_one_saves_as_read() {
    let nested = |items: Value| {
        json!({"allOf": [{"properties": {"d": {"type": "array",
            "items": {"type": "array", "items": items}}}}]})
    };
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        nested(json!({"enum": [null]})),
        json!({"d": ""}),
        json!({"d": "null"}),
    );
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        nested(json!({"type": "integer"})),
        json!({"d": "a"}),
        json!({"d": "7"}),
    );
    // Refused read either way (too few items), it keeps the host's type.
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"allOf": [{"properties": {"d": {"type": "array",
            "items": {"type": "array", "items": {"enum": [null]}, "minItems": 2}}}}]}),
        json!({"d": ""}),
        json!({"d": "null"}),
    );
}

/// An XML view left as shown, where the schema reads a host scalar's
/// element as a list of one (`<a>false</a>` where `a` is a list of
/// strings), saves nothing: the scalar stands where its source stands.
#[test]
fn an_unchanged_lone_element_the_schema_reads_as_a_list_saves_nothing() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"properties": {"a": {"type": "array", "items": {"type": "string"}}}}),
        json!({"a": false}),
        json!({"a": false}),
    );
}

/// A value typed in a view of an XML host that the host can write only
/// as text its schema reads as another value (`3` where an option taking
/// any text and an `enum` of numbers make `3` match both options of a
/// `oneOf`, so the text reads as the string `"3"`) waits: the save would
/// read back otherwise than the view, in every view.
#[test]
fn a_value_an_xml_host_reads_back_as_another_waits() {
    holds_but_known(
        "edit_as",
        super::edit_as::check,
        (
            json!({"properties": {"mode": {"oneOf": [
                {"properties": {"d": {"type": "boolean"}}},
                {"enum": [1, 2, 3]}]}}}),
            json!({"mode": 2.5}),
            json!({"mode": 3}),
        ),
        4,
    );
}

/// XML text the schema refuses in both readings, where the refusal of
/// the view's reading is an `if` beside it typing another value (`a: 1`
/// makes `b` a string), keeps the host's type: the documented kept type.
#[test]
fn text_an_if_beside_it_refuses_in_the_views_reading_keeps_the_hosts_type() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"properties": {"a": {"enum": ["a", 1]}, "b": {"type": "integer"}},
            "if": {"properties": {"a": {"const": 1}}, "required": ["a"]},
            "then": {"properties": {"b": {"type": "string"}}}}),
        json!({"a": "a", "b": 5}),
        json!({"a": 1, "b": 5}),
    );
}

/// A number typed in another spelling (`08`) inside a host's list of one
/// that the XML view shows as its item, where no schema reads the host's
/// list (properties of an object, the list kept), keeps the host's text
/// type with the text typed (`"08"`, not `"8"`): the documented kept list.
#[test]
fn a_number_respelled_in_a_list_of_one_shown_as_its_item_keeps_the_text_typed() {
    for of in ["oneOf", "anyOf", "allOf"] {
        for typed in ["08", "2.50", "1e3"] {
            holds_everywhere_but_known(
                "edit_as",
                super::edit_as::check,
                json!({"properties": {"x": {of: [{"properties": {"c": {"type": "number"}}}]}}}),
                json!({"x": [{"c": "n"}]}),
                json!({"x": [{"c": typed}]}),
            );
        }
    }
}

/// A host scalar left in place that the XML view showed as its host
/// value, and reads otherwise once a key beside it is removed (`b` typed
/// `c` a number, so `<c>true</c>` was `true`; without it `true` matches
/// both options of a `oneOf` and the text `"true"` one), saves as the
/// view now reads it where the schema prefers that: the host has a
/// problem exactly when the view shows one. Through a dependent schema
/// and an `if`, for a boolean and a number.
#[test]
fn a_scalar_the_view_reads_otherwise_after_an_edit_beside_it_saves_as_the_view_reads_it() {
    for (value, option) in [
        (json!(true), json!({"type": "boolean"})),
        (json!(8), json!({"type": "number"})),
    ] {
        let either = json!({"oneOf": [{"anyOf": [{}]}, option]});
        let number = json!({"properties": {"c": {"type": "number"}}});
        for beside in [
            json!({"dependentSchemas": {"b": number}}),
            json!({"if": {"required": ["b"]}, "then": number}),
        ] {
            let mut schema = json!({"properties": {"c": either}});
            schema
                .as_object_mut()
                .unwrap()
                .extend(beside.as_object().unwrap().clone());
            holds_everywhere_but_known(
                "edit_as",
                super::edit_as::check,
                schema,
                json!({"b": false, "c": value}),
                json!({"c": value}),
            );
        }
    }
}

/// A host scalar whose element the XML view writes again as a list's
/// first item, beside a new item, keeps its host type where the item's
/// schema refuses both readings (`false` and `"false"` where items are
/// `s`): the documented kept type, judged at the scalar the item came
/// from. Across the scalar's kinds; where items are text, the host's
/// type is refused and the view's reading saved.
#[test]
fn a_host_scalar_written_again_as_a_list_item_keeps_its_type_where_the_schema_tells_nothing() {
    for (host, items) in [
        (json!(false), json!({"enum": ["s"]})),
        (json!(0), json!({"enum": ["s"]})),
        (json!(1.5), json!({"enum": ["s"]})),
        (json!(false), json!({"type": "string"})),
        (json!(0), json!({"type": "string"})),
    ] {
        holds_everywhere_but_known(
            "edit_as",
            super::edit_as::check,
            json!({"properties": {"x": {"properties": {"b": {"items": items}}}}}),
            json!({"x": {"b": host}}),
            json!({"x": {"b": [host, "s"]}}),
        );
    }
}

/// A host item an XML view shows as the editor wrote it, moved to another
/// index by an item typed before it (`["true"]` typed `<a>a</a><a>true</a>`),
/// is that host item: the save keeps its host type where the schema at its
/// new index takes it (the view's text for it is the same either way), and
/// writes the view's reading where the schema there refuses the host's.
#[test]
fn a_host_item_moved_by_an_item_typed_before_it_saves_as_the_view_reads_it() {
    holds_everywhere_but_known(
        "edit_as",
        super::edit_as::check,
        json!({"properties": {"a": {"type": "array", "prefixItems": [{"type": "string"}]}}}),
        json!({"a": ["true"]}),
        json!({"a": ["a", true]}),
    );
}

/// [`a_host_item_moved_by_an_item_typed_before_it_saves_as_the_view_reads_it`]
/// across the schemas that type an index (`prefixItems`, `items`, a tuple
/// of two that takes or refuses the host's type, none) and scalar kinds.
#[test]
fn a_host_item_moved_to_another_index_saves_as_the_view_reads_it_whatever_types_the_index() {
    let text = json!({"type": "string"});
    for list in [
        json!({"type": "array", "prefixItems": [text.clone()]}),
        json!({"type": "array", "items": text.clone()}),
        json!({"type": "array", "prefixItems": [text.clone(), text.clone()]}),
        json!({"type": "array", "prefixItems": [text.clone(), {"type": "boolean"}]}),
        json!({"type": "array", "prefixItems": [text.clone(), {"type": "integer"}]}),
        json!({"type": "array"}),
        json!({}),
    ] {
        for (host, typed) in [
            (json!("true"), json!(true)),
            (json!("8"), json!(8)),
            (json!(true), json!(true)),
            (json!(8), json!(8)),
        ] {
            holds_everywhere_but_known(
                "edit_as",
                super::edit_as::check,
                json!({"properties": {"a": list.clone()}}),
                json!({"a": [host]}),
                json!({"a": ["a", typed]}),
            );
        }
    }
}
