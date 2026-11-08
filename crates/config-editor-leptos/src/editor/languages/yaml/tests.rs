use super::super::super::completion::{
    accept_marked, accept_reopens_suggestions, apply_completion,
    completions as shared_completions, key_hover as shared_key_hover,
};
use super::super::super::diagnostics::{
    Diagnostic, SchemaBundle, build_schema, validate as shared_validate,
};
use super::super::super::keyboard::paired_edit as shared_paired_edit;
use super::super::super::language::ConfigLanguage;
use super::YamlBehavior;
use super::super::super::highlight::{TokenKind, runs};
use super::{highlight, scalar_kind, split_yaml_comment};

fn validate_yaml(
    source: &str,
    schema: Option<&SchemaBundle>,
    schema_error: Option<&str>,
    enabled: bool,
) -> Vec<Diagnostic> {
    shared_validate(source, schema, schema_error, enabled, &YamlBehavior)
}

fn flow_schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{"web":{"type":"object","properties":{
            "http":{"type":"integer"},
            "hosts":{"type":"array","items":{"type":"string"}},
            "letsencrypt":{"type":"object","required":["email"],
                "properties":{"email":{"type":"string"}}},
            "headers":{"type":"object"}}}}}"#,
    )
    .unwrap()
}

#[test]
fn an_object_accepted_inside_a_flow_mapping_stays_in_flow_style() {
    let schema = flow_schema();
    let accepted = accept_marked("web: { le| }\n", "letsencrypt", &schema, &YamlBehavior);
    assert_eq!(accepted, "web: { letsencrypt: { email: | } }\n");
    let filled = accepted.replace('|', "a@b.c");
    let document = YamlBehavior.parse(&filled).expect("flow mapping parses");
    assert_eq!(document["web"]["letsencrypt"]["email"], "a@b.c");
}

#[test]
fn an_array_and_an_empty_object_inside_a_flow_mapping_open_inline() {
    let schema = flow_schema();
    assert_eq!(
        accept_marked("web: { ho| }\n", "hosts", &schema, &YamlBehavior),
        "web: { hosts: [|] }\n"
    );
    assert_eq!(
        accept_marked("web: { he| }\n", "headers", &schema, &YamlBehavior),
        "web: { headers: { | } }\n"
    );
}

#[test]
fn an_object_accepted_in_a_block_mapping_still_expands_to_a_block() {
    let schema = flow_schema();
    assert_eq!(
        accept_marked("web:\n  le|", "letsencrypt", &schema, &YamlBehavior),
        "web:\n  letsencrypt:\n    email: |\n"
    );
}

fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
) -> Vec<super::super::super::completion::Completion> {
    shared_completions(source, cursor, force, schema, enabled, &YamlBehavior)
}

fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
) -> Option<super::super::super::completion::KeyHover> {
    shared_key_hover(source, line_index, column, schema, &YamlBehavior)
}

fn paired_edit(
    source: &str,
    start: usize,
    end: usize,
    opening: char,
) -> Option<(String, usize, usize)> {
    shared_paired_edit(source, start, end, opening, &YamlBehavior)
}

fn newline_with_indent(source: &str, cursor: usize) -> String {
    YamlBehavior.newline(source, cursor).text
}

fn toggle_yaml_line_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
) -> Option<(String, usize, usize)> {
    YamlBehavior.toggle_comments(source, selection_start, selection_end)
}

fn test_schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{"public_address":{"type":"string","description":"Address"},"websocket":{"type":"object","properties":{"challenge":{"enum":["tls","http","dns"]}}}},"additionalProperties":false}"#,
    )
    .unwrap()
}

/// Nullable objects, `$ref` branches, dynamic `additionalProperties` keys,
/// enums, and numeric-as-string examples.
#[cfg(feature = "schema-validation")]
fn fixture_schema() -> SchemaBundle {
    build_schema(include_str!("../../../../tests/fixtures/schema.json")).unwrap()
}

#[test]
fn paired_quotes_and_brackets_wrap_selected_text_and_keep_it_selected() {
    for (opening, expected) in [
        ('"', "\"value\""),
        ('\'', "'value'"),
        ('(', "(value)"),
        ('[', "[value]"),
        ('{', "{value}"),
    ] {
        let (updated, selection_start, selection_end) =
            paired_edit("value", 0, 5, opening).unwrap();
        assert_eq!(updated, expected);
        assert_eq!((selection_start, selection_end), (1, 6));
    }
}

#[test]
fn paired_brackets_leave_the_cursor_between_the_pair() {
    let (updated, selection_start, selection_end) = paired_edit("a b", 1, 1, '[').unwrap();
    assert_eq!(updated, "a[] b");
    assert_eq!((selection_start, selection_end), (2, 2));
    let (updated, _, _) = paired_edit("key: ", 5, 5, '{').unwrap();
    assert_eq!(updated, "key: {}");
}

#[test]
fn a_pair_does_not_auto_close_before_a_word_character() {
    assert_eq!(paired_edit("ab", 1, 1, '['), None);
    assert_eq!(paired_edit("foo", 0, 0, '"'), None);
    let (updated, _, _) = paired_edit("[]", 1, 1, '"').unwrap();
    assert_eq!(updated, "[\"\"]");
}

#[test]
fn a_quote_after_a_word_character_does_not_pair() {
    assert_eq!(paired_edit("msg: don", 8, 8, '\''), None);
    assert_eq!(paired_edit("name_", 5, 5, '"'), None);
    let (updated, _, _) = paired_edit("msg: ", 5, 5, '\'').unwrap();
    assert_eq!(updated, "msg: ''");
}

#[test]
fn an_opening_bracket_pairs_even_before_an_outer_closer() {
    let (updated, selection_start, selection_end) = paired_edit(")", 0, 0, '(').unwrap();
    assert_eq!(updated, "())");
    assert_eq!((selection_start, selection_end), (1, 1));
}

#[test]
fn paired_quote_edit_respects_escaped_context_and_utf16_selections() {
    assert_eq!(paired_edit("path\\", 5, 5, '"'), None);
    let (updated, selection_start, selection_end) = paired_edit("🙂x", 0, 3, '"').unwrap();
    assert_eq!(updated, "\"🙂x\"");
    assert_eq!((selection_start, selection_end), (1, 4));
}

#[test]
fn pairing_does_not_insert_inside_quoted_scalars_or_comments() {
    assert_eq!(paired_edit("name: \"value\"", 8, 8, '['), None);
    assert_eq!(paired_edit("name: plain # comment", 17, 17, '['), None);
}

#[test]
fn valid_yaml_is_accepted_by_the_provided_schema() {
    assert!(
        validate_yaml(
            "public_address: example.test\n",
            Some(&test_schema()),
            None,
            true
        )
        .is_empty()
    );
}

#[test]
fn syntax_errors_include_the_yaml_line() {
    let errors = validate_yaml("value: [", None, None, true);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, Some(1));
    assert!(errors[0].column.is_some());
    assert!(!errors[0].message.is_empty());
}

#[test]
fn an_empty_document_reports_no_diagnostics() {
    for source in ["", "\n", "  \n  \n"] {
        assert!(
            validate_yaml(source, Some(&test_schema()), None, true).is_empty(),
            "{source:?} should not report diagnostics"
        );
    }
}

#[test]
fn duplicate_mapping_keys_are_reported_on_the_repeated_line() {
    let errors = validate_yaml(
        "insecure_plaintext: false\ninsecure_plaintext: true\n",
        None,
        None,
        true,
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, Some(2));
    assert!(errors[0].message.contains("duplicate"));
}

#[test]
fn duplicate_keys_are_detected_in_flow_mappings_and_quoted_forms() {
    for (source, expected_line) in [
        ("one: 1\n\"one\": 2\n", 2),
        ("settings: {mode: safe, mode: fast}\n", 1),
    ] {
        let errors = validate_yaml(source, None, None, true);
        assert_eq!(errors.len(), 1, "{source}");
        assert_eq!(errors[0].line, Some(expected_line), "{source}");
        assert!(errors[0].message.contains("duplicate"), "{source}");
    }
}

#[test]
fn malformed_schema_is_reported_as_a_diagnostic() {
    let schema_error = build_schema("not-json").err().unwrap();
    let errors = validate_yaml("name: app\n", None, Some(&schema_error), true);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.contains("Invalid JSON Schema"),
        "{}",
        errors[0].message
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn schema_errors_are_reported_for_unknown_properties() {
    let errors = validate_yaml("unknown_option: true\n", Some(&test_schema()), None, true);
    assert!(!errors.is_empty());
    assert_eq!(errors[0].line, Some(1));
    assert_eq!(errors[0].column, Some(1));
    assert!(errors[0].end_column.unwrap() > errors[0].column.unwrap());
}

#[cfg(feature = "schema-validation")]
#[test]
fn nested_schema_errors_map_to_the_yaml_key_range() {
    let source = "websocket:\n  challenge: invalid\n";
    let errors = validate_yaml(source, Some(&test_schema()), None, true);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, Some(2));
    assert_eq!(errors[0].column, Some(14));
    assert_eq!(errors[0].end_column, Some(21));
}

#[cfg(feature = "schema-validation")]
#[test]
fn top_level_property_suggestions_come_from_the_schema() {
    let input = "pub";
    let schema = test_schema();
    let options = completions(input, 3, false, Some(&schema), true);
    let option = options
        .iter()
        .find(|option| option.label == "public_address")
        .unwrap();
    assert!(!option.is_value);
    assert_eq!(option.type_label, "string");
    assert_eq!(option.detail, "Address");
}

#[cfg(feature = "schema-validation")]
#[test]
fn explicit_completion_request_offers_schema_properties_at_an_empty_position() {
    let options = completions("", 0, true, Some(&test_schema()), true);
    assert!(!options.is_empty());
    assert!(
        options
            .iter()
            .any(|option| option.label == "public_address")
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn property_completions_omit_keys_already_present_in_the_mapping() {
    let input = "public_address: existing\n";
    let options = completions(
        input,
        input.encode_utf16().count(),
        true,
        Some(&test_schema()),
        true,
    );
    assert!(
        !options
            .iter()
            .any(|option| option.label == "public_address")
    );
    assert!(options.iter().any(|option| option.label == "websocket"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn partially_typed_keys_do_not_reoffer_existing_keys_when_yaml_is_temporarily_invalid() {
    let schema = fixture_schema();
    let input = "listen: 7835\ninsecure_plaintext: false\nins";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert!(
        !options
            .iter()
            .any(|option| option.label == "insecure_plaintext")
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn cursor_before_a_mapping_colon_is_treated_as_key_completion_without_panicking() {
    let schema = fixture_schema();
    let input = "insecure_plaintext: false";
    let cursor = "ins".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "insecure_plaintext")
        .expect("partial key should be completed as a property, not a boolean value");
    assert_eq!(apply_completion(input, &option).0, input);
}

#[cfg(feature = "schema-validation")]
#[test]
fn enum_suggestions_are_available_for_nested_values() {
    let input = "websocket:\n  challenge: t";
    let schema = test_schema();
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let option = options.iter().find(|option| option.label == "tls").unwrap();
    assert!(option.is_value);
    // The values' type; the closed set shows in the suggestions themselves.
    assert_eq!(option.type_label, "string");
}

#[cfg(feature = "schema-validation")]
#[test]
fn property_completion_displays_combined_schema_types() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"name":{"anyOf":[{"type":"string"},{"type":"null"}]}}}"#,
    )
    .unwrap();
    let options = completions("", 0, true, Some(&schema), true);
    let option = options
        .iter()
        .find(|option| option.label == "name")
        .unwrap();
    assert_eq!(option.type_label, "null | string");
}

#[cfg(feature = "schema-validation")]
#[test]
fn property_completion_quotes_keys_that_need_yaml_quoting() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"display:name":{"type":"string"},"true":{"type":"boolean"}}}"#,
    )
    .unwrap();
    let options = completions("", 0, true, Some(&schema), true);
    assert_eq!(
        options
            .iter()
            .find(|option| option.label == "display:name")
            .unwrap()
            .insert,
        "\"display:name\": "
    );
    assert_eq!(
        options
            .iter()
            .find(|option| option.label == "true")
            .unwrap()
            .insert,
        "\"true\": "
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn object_completion_expands_required_fields_and_places_cursor_at_first_value() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"auth":{"type":"object","required":["enabled","token","mode"],"properties":{"enabled":{"type":"boolean","default":false},"token":{"type":"string"},"mode":{"type":"string","default":"safe"}}}}}"#,
    )
    .unwrap();
    let options = completions("", 0, true, Some(&schema), true);
    let option = options
        .iter()
        .find(|option| option.label == "auth")
        .unwrap();
    assert_eq!(
        option.insert,
        "auth:\n  enabled: false\n  token: \n  mode: safe\n"
    );
    let (updated, _, cursor) = apply_completion("", option);
    assert_eq!(updated, option.insert);
    assert_eq!(
        cursor,
        "auth:\n  enabled: false\n  token: ".encode_utf16().count()
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn completions_follow_additional_properties_into_dynamic_object_values() {
    let schema = build_schema(
        r##"{
            "type":"object",
            "properties":{
                "clients":{
                    "type":"object",
                    "default":{},
                    "additionalProperties":{
                        "$ref":"#/$defs/Client"
                    }
                }
            },
            "$defs":{
                "Client":{
                    "type":"object",
                    "properties":{
                        "secret":{"type":"string"},
                        "max_connections":{"type":"integer"}
                    }
                }
            }
        }"##,
    )
    .unwrap();
    let root_options = completions("", 0, true, Some(&schema), true);
    assert_eq!(
        root_options
            .iter()
            .find(|option| option.label == "clients")
            .unwrap()
            .insert,
        "clients:\n  "
    );
    let input = "clients:\n  alex:\n    ";
    let options = completions(
        input,
        input.encode_utf16().count(),
        true,
        Some(&schema),
        true,
    );
    assert!(options.iter().any(|option| option.label == "secret"));
    assert!(
        options
            .iter()
            .any(|option| option.label == "max_connections")
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn fixture_schema_covers_dynamic_client_keys_through_additional_properties() {
    let schema = fixture_schema();
    let input = "clients:\n  alex:\n    ";
    let options = completions(
        input,
        input.encode_utf16().count(),
        true,
        Some(&schema),
        true,
    );
    assert!(options.iter().any(|option| option.label == "secret"));
    assert!(
        options
            .iter()
            .any(|option| option.label == "max_connections")
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn completions_follow_pattern_properties_and_quoted_mapping_keys() {
    let schema = build_schema(
        r##"{
            "type":"object",
            "patternProperties":{
                "^client:":{
                    "type":"object",
                    "properties":{"enabled":{"type":"boolean"}}
                }
            }
        }"##,
    )
    .unwrap();
    let input = "\"client:one\":\n  en";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert!(options.iter().any(|option| option.label == "enabled"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn array_item_completions_follow_items_and_prefix_items_schemas() {
    let schema = build_schema(
        r#"{
            "type":"object",
            "properties":{
                "clients":{
                    "type":"array",
                    "items":{
                        "type":"object",
                        "properties":{"name":{"type":"string"}}
                    }
                },
                "modes":{
                    "type":"array",
                    "default":[],
                    "prefixItems":[{"enum":["fast","safe"]}]
                }
            }
        }"#,
    )
    .unwrap();

    let root_options = completions("", 0, true, Some(&schema), true);
    assert_eq!(
        root_options
            .iter()
            .find(|option| option.label == "modes")
            .unwrap()
            .insert,
        "modes:\n  - "
    );

    let object_input = "clients:\n  - na";
    let object_options = completions(
        object_input,
        object_input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert!(object_options.iter().any(|option| option.label == "name"));

    let enum_input = "modes:\n  - fa";
    let enum_options = completions(
        enum_input,
        enum_input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert!(enum_options.iter().any(|option| option.label == "fast"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn const_and_boolean_schema_values_are_suggested_and_inserted() {
    let schema = build_schema(
        r#"{
            "type":"object",
            "properties":{
                "enabled":{"const":true},
                "secure":{"type":"boolean"}
            }
        }"#,
    )
    .unwrap();
    let key_options = completions("", 0, true, Some(&schema), true);
    assert_eq!(
        key_options
            .iter()
            .find(|option| option.label == "enabled")
            .unwrap()
            .insert,
        "enabled: true"
    );

    let value_input = "secure: t";
    let value_options = completions(
        value_input,
        value_input.encode_utf16().count(),
        true,
        Some(&schema),
        true,
    );
    assert!(value_options.iter().any(|option| option.label == "true"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn examples_are_suggested_when_asked_or_empty_but_not_while_typing() {
    let schema = build_schema(
        r#"{
            "type":"object",
            "properties":{
                "region":{"type":"string","examples":["eu-west-1","us-east-1"]},
                "legacy":{"type":"string","example":"old.example"}
            }
        }"#,
    )
    .unwrap();
    let labels = |input: &str, force: bool| -> Vec<String> {
        completions(input, input.encode_utf16().count(), force, Some(&schema), true)
            .into_iter()
            .map(|option| option.label)
            .collect()
    };
    // Asked for (Ctrl+Space) or with nothing typed yet, they are listed.
    assert!(labels("region: eu", true).contains(&"eu-west-1".to_string()));
    assert!(labels("legacy: ", true).contains(&"old.example".to_string()));
    // While the user types their own value they are not, so Enter keeps it.
    assert!(labels("region: eu", false).is_empty());
}

#[cfg(feature = "schema-validation")]
#[test]
fn nullable_type_arrays_expand_objects_and_offer_null_values() {
    let schema = build_schema(
        r#"{
            "type":"object",
            "properties":{
                "cache":{
                    "type":["object","null"],
                    "properties":{
                        "size":{"type":"integer"},
                        "ttl":{"type":"integer"}
                    }
                }
            }
        }"#,
    )
    .unwrap();

    let key_options = completions("", 0, true, Some(&schema), true);
    assert_eq!(
        key_options
            .iter()
            .find(|option| option.label == "cache")
            .unwrap()
            .insert,
        "cache:\n  "
    );

    let value_input = "cache: n";
    let value_options = completions(
        value_input,
        value_input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert!(value_options.iter().any(|option| option.label == "null"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn fuzzy_completion_matches_word_initials_and_subsequences() {
    let schema = build_schema(
        r#"{
            "type":"object",
            "properties":{
                "max_connections":{"type":"integer"},
                "min_connections":{"type":"integer"}
            }
        }"#,
    )
    .unwrap();

    let input = "maxc";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert_eq!(
        options.first().map(|option| option.label.as_str()),
        Some("max_connections")
    );
    assert!(
        !options
            .iter()
            .any(|option| option.label == "min_connections")
    );

    let initials = "mc";
    let initials_options = completions(
        initials,
        initials.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert_eq!(initials_options.len(), 2);
}

#[cfg(feature = "schema-validation")]
#[test]
fn quoted_partial_keys_are_replaced_with_the_quoted_schema_key() {
    let schema =
        build_schema(r#"{"type":"object","properties":{"display:name":{"type":"string"}}}"#)
            .unwrap();
    let input = "\"disp";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let option = options
        .iter()
        .find(|option| option.label == "display:name")
        .unwrap();
    assert_eq!(option.insert, "\"display:name\": ");
    let (updated, _, cursor) = apply_completion(input, option);
    assert_eq!(updated, "\"display:name\": ");
    assert_eq!(cursor, updated.encode_utf16().count());
}

#[cfg(feature = "schema-validation")]
#[test]
fn expanded_completions_follow_the_document_indentation_unit() {
    let schema = build_schema(
        r#"{
            "type":"object",
            "properties":{
                "server":{
                    "type":"object",
                    "properties":{
                        "auth":{
                            "type":"object",
                            "required":["enabled","token"],
                            "properties":{
                                "enabled":{"type":"boolean","default":false},
                                "token":{"type":"string"}
                            }
                        }
                    }
                }
            }
        }"#,
    )
    .unwrap();

    let input = "server:\n    ";
    let options = completions(
        input,
        input.encode_utf16().count(),
        true,
        Some(&schema),
        true,
    );
    let option = options
        .iter()
        .find(|option| option.label == "auth")
        .unwrap();
    // A line of only spaces does not set the unit (it would flip while
    // typing), so children go one default unit deeper.
    assert_eq!(
        option.insert,
        "auth:\n      enabled: false\n      token: \n"
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn typing_on_a_blank_line_suggests_missing_top_level_keys() {
    let schema = fixture_schema();
    let input = "listen: 7835\n\nc";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    assert!(
        labels.contains(&"clients"),
        "typing 'c' on a blank line should suggest missing keys, got: {labels:?}"
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn typing_after_a_fresh_indent_suggests_nested_keys() {
    let schema = fixture_schema();
    let input = "web:\n  h";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    assert!(
        labels.contains(&"http") || labels.contains(&"https"),
        "typing after a fresh indent should suggest nested keys, got: {labels:?}"
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn empty_query_on_a_fresh_indent_stays_silent_without_force() {
    let schema = fixture_schema();
    let input = "clients:\n  alex:\n    ";
    let cursor = input.encode_utf16().count();
    assert!(
        completions(input, cursor, false, Some(&schema), true).is_empty(),
        "empty query without Ctrl+Space should stay silent (VS Code parity)"
    );
    assert!(
        !completions(input, cursor, true, Some(&schema), true).is_empty(),
        "Ctrl+Space on a fresh indent should list nested keys"
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn fixture_web_http_value_completion_stays_on_the_same_line() {
    let schema = fixture_schema();
    let input = "web:\n  http: ";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let option = options
        .iter()
        .find(|option| option.label == "0.0.0.0:7835")
        .expect("Bind examples should be offered for web.http");
    let (updated, _, cursor) = apply_completion(input, option);
    assert_eq!(updated, "web:\n  http: \"0.0.0.0:7835\"");
    assert_eq!(cursor, updated.encode_utf16().count());
}

#[test]
fn enter_continues_yaml_indentation_and_nests_after_a_mapping_key() {
    assert_eq!(newline_with_indent("services:", 9), "\n  ");
    assert_eq!(newline_with_indent("services:\n  web: ", 17), "\n    ");
}

#[test]
fn enter_aligns_sequence_item_mappings_and_preserves_spacing_after_the_dash() {
    let source = "services:\n  - web:";
    assert_eq!(newline_with_indent(source, source.len()), "\n      ");

    let compact = "services:\n  - billing: 8080";
    assert_eq!(newline_with_indent(compact, compact.len()), "\n    ");

    let padded = "services:\n  -   web:";
    assert_eq!(newline_with_indent(padded, padded.len()), "\n        ");

    // Enter on an empty item ends the list: the line goes back to the
    // column of the key holding it.
    let empty_item = "services:\n  - ";
    let ended = YamlBehavior.newline(empty_item, empty_item.len());
    assert_eq!((ended.text.as_str(), ended.replaces_before), ("", 4));

    let plain_scalar_key = "services:\n  -web: 8080";
    assert_eq!(
        newline_with_indent(plain_scalar_key, plain_scalar_key.len()),
        "\n  "
    );
}

#[test]
fn comment_shortcut_comments_selected_lines_and_preserves_selection() {
    let source = "one: 1\ntwo: 2\n";
    let (updated, start, end) =
        toggle_yaml_line_comments(source, 0, source.encode_utf16().count()).unwrap();
    assert_eq!(updated, "# one: 1\n# two: 2\n");
    assert_eq!(start, 0);
    assert_eq!(end, updated.encode_utf16().count());
}

#[test]
fn comment_shortcut_uncomments_indented_lines_and_keeps_blank_lines() {
    let source = "  # first\n\n  # second\n";
    let (updated, _, _) =
        toggle_yaml_line_comments(source, 0, source.encode_utf16().count()).unwrap();
    assert_eq!(updated, "  first\n\n  second\n");
}

#[cfg(feature = "schema-validation")]
#[test]
fn accepting_a_value_closes_suggestions_but_accepting_a_key_reopens_them() {
    let schema = fixture_schema();
    let input = "web:\n  http: ";
    let value = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    )
    .into_iter()
    .find(|option| option.label == "0.0.0.0:7835")
    .expect("web.http example should be suggested");
    assert!(!accept_reopens_suggestions(&value, true));

    let key = completions(
        "web:\n  ",
        "web:\n  ".encode_utf16().count(),
        true,
        Some(&schema),
        true,
    )
    .into_iter()
    .find(|option| option.label == "http")
    .expect("web.http key should be suggested");
    assert!(accept_reopens_suggestions(&key, true));
    assert!(!accept_reopens_suggestions(&key, false));
}

#[cfg(feature = "schema-validation")]
#[test]
fn value_directly_after_a_bare_colon_gets_a_separating_space() {
    let schema = fixture_schema();
    let input = "web:\n  http:";
    let option = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    )
    .into_iter()
    .find(|option| option.label == "0.0.0.0:7835")
    .expect("web.http example should be suggested after a bare colon");
    let (updated, _, cursor) = apply_completion(input, &option);
    assert_eq!(updated, "web:\n  http: \"0.0.0.0:7835\"");
    assert_eq!(cursor, updated.encode_utf16().count());
}

#[cfg(feature = "schema-validation")]
#[test]
fn hovering_a_key_returns_its_schema_documentation() {
    let schema = fixture_schema();
    let source = "web:\n  http: 80\n";
    let web = key_hover(source, 0, 1, Some(&schema)).expect("web should have docs");
    assert_eq!(web.key, "web");
    assert_eq!(
        web.description.as_deref(),
        Some("The web proxy settings; off unless set")
    );

    let http = key_hover(source, 1, 4, Some(&schema)).expect("nested http should have docs");
    assert_eq!(http.key, "http");
    assert_eq!(http.start_column, 2);
    assert_eq!(
        http.examples,
        vec!["0.0.0.0:7835".to_string(), "7835".to_string()]
    );
    assert!(http.type_label.contains("integer") || http.type_label.contains("string"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn hovering_outside_a_key_or_on_an_unknown_key_shows_nothing() {
    let schema = fixture_schema();
    let source = "web:\n  http: 80\n  unknown_key: 1\n";
    assert!(
        key_hover(source, 1, 8, Some(&schema)).is_none(),
        "value text is not a key"
    );
    assert!(
        key_hover(source, 2, 3, Some(&schema)).is_none(),
        "unknown keys have no schema"
    );
    assert!(
        key_hover(source, 0, 1, None).is_none(),
        "no schema, no docs"
    );
}

#[test]
fn hover_marks_required_properties_from_the_parent_schema() {
    let schema = build_schema(
        r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","description":"App name"},"port":{"type":"integer","default":8080}}}"#,
    )
    .unwrap();
    let name = key_hover("name: app\n", 0, 0, Some(&schema)).unwrap();
    assert!(name.required);
    assert_eq!(name.description.as_deref(), Some("App name"));
    let port = key_hover("port: 1\n", 0, 0, Some(&schema)).unwrap();
    assert!(!port.required);
    assert_eq!(port.default.as_deref(), Some("8080"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn null_is_offered_only_when_explicitly_requested_or_typed() {
    let schema = fixture_schema();
    let auto = completions("web:", 4, false, Some(&schema), true);
    assert!(
        auto.iter().all(|option| option.label != "null"),
        "auto-triggered list after a bare key must not offer null"
    );
    let explicit = completions("web:", 4, true, Some(&schema), true);
    assert!(explicit.iter().any(|option| option.label == "null"));
    let typed = completions("listen: nu", 10, false, Some(&schema), true);
    assert!(typed.iter().any(|option| option.label == "null"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn numeric_examples_are_inserted_as_numbers_when_the_schema_allows_integers() {
    let schema = fixture_schema();
    let input = "listen: ";
    let port = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    )
    .into_iter()
    .find(|option| option.label == "7835")
    .expect("port example should be suggested");
    let (updated, _, _) = apply_completion(input, &port);
    assert_eq!(updated, "listen: 7835");
}

#[cfg(feature = "schema-validation")]
#[test]
fn enum_words_are_inserted_as_plain_scalars_and_ambiguous_strings_stay_quoted() {
    use crate::editor::languages::yaml::yaml_value;
    use serde_json::Value;
    let schema = fixture_schema();
    let input = "services:\n  billing:\n    protocol: ";
    let udp = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    )
    .into_iter()
    .find(|option| option.label == "udp")
    .expect("protocol values should be suggested");
    assert_eq!(
        apply_completion(input, &udp).0,
        "services:\n  billing:\n    protocol: udp"
    );
    assert_eq!(yaml_value(&Value::String("yes".into())), "\"yes\"");
    assert_eq!(yaml_value(&Value::String("7835".into())), "\"7835\"");
    assert_eq!(
        yaml_value(&Value::String("0.0.0.0:7835".into())),
        "\"0.0.0.0:7835\""
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn required_properties_sort_before_optional_ones() {
    let schema = fixture_schema();
    let input = "services:\n  dashboard:\n    ";
    let options = completions(
        input,
        input.encode_utf16().count(),
        true,
        Some(&schema),
        true,
    );
    assert_eq!(
        options.first().map(|option| option.label.as_str()),
        Some("target")
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn value_for_an_expanded_required_key_stays_on_its_line() {
    let schema = fixture_schema();
    // What accepting `metrics` inserts.
    let input = "metrics:\n  listen: \n";
    let cursor = "metrics:\n  listen: ".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "0.0.0.0:7835")
        .expect("metrics.listen should offer Bind examples");
    let (updated, _, cursor_after) = apply_completion(input, &option);
    assert_eq!(updated, "metrics:\n  listen: \"0.0.0.0:7835\"\n");
    assert_eq!(
        cursor_after,
        "metrics:\n  listen: \"0.0.0.0:7835\""
            .encode_utf16()
            .count()
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn typing_inside_a_value_uses_the_text_before_the_caret_and_replaces_the_token() {
    let schema = fixture_schema();
    let input = "update:\n  remote: ftrue";
    let cursor = "update:\n  remote: f".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "false")
        .expect("typing before existing text should suggest the matching value");
    let (updated, _, cursor_after) = apply_completion(input, &option);
    assert_eq!(updated, "update:\n  remote: false");
    assert_eq!(cursor_after, updated.encode_utf16().count());
}

#[cfg(feature = "schema-validation")]
#[test]
fn accepting_a_value_keeps_an_inline_comment() {
    let schema = fixture_schema();
    let input = "update:\n  remote: tru # keep me";
    let cursor = "update:\n  remote: tru".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "true")
        .expect("true should match the partial value");
    let (updated, _, _) = apply_completion(input, &option);
    assert_eq!(updated, "update:\n  remote: true # keep me");
}

#[cfg(feature = "schema-validation")]
#[test]
fn completion_replace_range_survives_multibyte_characters_before_the_caret() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"café":{"type":"string"},"listen":{"type":"integer"}}}"#,
    )
    .unwrap();
    // `é` is one UTF-16 unit but two UTF-8 bytes.
    let input = "café: 1\nlis: 2";
    let cursor = "café: 1\nlis".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "listen")
        .expect("partial key should complete to the schema property");
    let (updated, _, _) = apply_completion(input, &option);
    assert_eq!(updated, "café: 1\nlisten: 2");
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_nested_key_maps_to_its_own_line_not_the_first_key_with_the_same_name() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"listen":{"type":"integer"},"websocket":{"type":"object","properties":{"listen":{"type":"integer","minimum":1}}}}}"#,
    )
    .unwrap();
    let source = "listen: 7835\nwebsocket:\n  listen: 0\n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(3));
    assert_eq!(diagnostics[0].column, Some(11));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_unexpected_nested_key_maps_to_its_own_line_not_a_root_key_with_the_same_name() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"listen":{"type":"integer"},"websocket":{"type":"object","properties":{},"additionalProperties":false}}}"#,
    )
    .unwrap();
    let source = "listen: 7835\nwebsocket:\n  listen: 0\n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(3));
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_key_inside_a_sequence_item_maps_to_its_own_line() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"weight":{"type":"integer"},"services":{"type":"object","additionalProperties":{"type":"object","properties":{"clients":{"type":"array","items":{"type":"object","properties":{"weight":{"type":"integer","minimum":1}}}}}}}}}"#,
    )
    .unwrap();
    let source = "weight: 5\nservices:\n  billing:\n    clients:\n      - name: laptop\n        weight: 0\n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(6));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_empty_sequence_item_error_points_to_the_dash_line() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"self":{"type":"object","properties":{"allow":{"type":"array","items":{"type":"string"}}}}}}"#,
    )
    .unwrap();
    let source = "self:\n  allow:\n    - \n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(3));
    assert_eq!(diagnostics[0].column, Some(7));
}

#[cfg(feature = "schema-validation")]
#[test]
fn empty_schema_value_points_after_the_colon_instead_of_at_the_key() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"websocket":{"type":"object","properties":{"listen":{"type":"string"}}}}}"#,
    )
    .unwrap();
    let source = "websocket:\n  listen: \n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
    assert_eq!(diagnostics[0].column, Some(11));
    assert_eq!(diagnostics[0].end_column, Some(12));
}

#[cfg(feature = "schema-validation")]
#[test]
fn empty_schema_value_with_comment_points_to_the_value_slot() {
    let schema =
        build_schema(r#"{"type":"object","properties":{"listen":{"type":"string"}}}"#).unwrap();
    let source = "listen: # set a port\n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
    assert_eq!(diagnostics[0].column, Some(9));
    assert_eq!(diagnostics[0].end_column, Some(10));
}

#[cfg(feature = "schema-validation")]
#[test]
fn nested_any_of_errors_point_to_the_specific_empty_value() {
    let schema = build_schema(
        r##"{
            "type":"object",
            "properties":{
                "websocket":{
                    "anyOf":[
                        {"$ref":"#/$defs/WebSocket"},
                        {"type":"null"}
                    ]
                }
            },
            "$defs":{
                "WebSocket":{
                    "type":"object",
                    "properties":{
                        "listen":{"anyOf":[{"type":"integer"},{"type":"string"}]}
                    },
                    "required":["listen"]
                }
            }
        }"##,
    )
    .unwrap();
    let source = "websocket:\n  listen: \n";

    let diagnostics = validate_yaml(source, Some(&schema), None, true);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
    assert_eq!(diagnostics[0].column, Some(11));
    assert!(diagnostics[0].message.contains("/websocket/listen"));
}

#[test]
fn plain_scalars_are_colored_as_strings_and_yaml_literals_stay_distinct() {
    assert_eq!(scalar_kind("unquoted value"), TokenKind::Value);
    assert_eq!(scalar_kind("\"quoted value\""), TokenKind::String);
    assert_eq!(scalar_kind("false"), TokenKind::Literal);
    assert_eq!(scalar_kind("42"), TokenKind::Literal);
}

fn highlighted(line: &str) -> Vec<(Option<TokenKind>, String)> {
    runs(line, &highlight(line))
}

fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
    (kind, text.to_string())
}

#[test]
fn a_comment_line_is_one_comment_run_after_its_indent() {
    assert_eq!(
        highlighted("  # note: x"),
        [run(None, "  "), run(Some(TokenKind::Comment), "# note: x")]
    );
}

#[test]
fn a_mapping_key_gets_key_and_colon_runs_then_its_value() {
    assert_eq!(
        highlighted("  name: app"),
        [
            run(None, "  "),
            run(Some(TokenKind::Key), "name"),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::Value), "app"),
        ]
    );
    assert_eq!(
        highlighted("parent:"),
        [
            run(Some(TokenKind::Key), "parent"),
            run(Some(TokenKind::Punctuation), ":"),
        ]
    );
}

#[test]
fn values_are_classed_as_literal_string_or_value() {
    let value_kind = |line: &str| highlighted(line).last().unwrap().clone();
    assert_eq!(value_kind("a: 42"), run(Some(TokenKind::Literal), "42"));
    assert_eq!(value_kind("a: ~"), run(Some(TokenKind::Literal), "~"));
    assert_eq!(value_kind("a: true"), run(Some(TokenKind::Literal), "true"));
    assert_eq!(
        value_kind("a: 'x y'"),
        run(Some(TokenKind::String), "'x y'")
    );
    assert_eq!(
        value_kind("a: \"x\""),
        run(Some(TokenKind::String), "\"x\"")
    );
    // A flow collection is colored entry by entry.
    assert_eq!(
        highlighted("a: [1, x]"),
        vec![
            run(Some(TokenKind::Key), "a"),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "["),
            run(Some(TokenKind::Literal), "1"),
            run(Some(TokenKind::Punctuation), ","),
            run(None, " "),
            run(Some(TokenKind::Value), "x"),
            run(Some(TokenKind::Punctuation), "]"),
        ]
    );
}

#[test]
fn an_inline_comment_after_a_value_is_its_own_run() {
    assert_eq!(
        highlighted("port: 80 # http"),
        [
            run(Some(TokenKind::Key), "port"),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::Literal), "80 "),
            run(Some(TokenKind::Comment), "# http"),
        ]
    );
    assert_eq!(
        highlighted("a: \"x # y\""),
        [
            run(Some(TokenKind::Key), "a"),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::String), "\"x # y\""),
        ]
    );
}

#[test]
fn a_dash_before_a_key_stays_plain_but_a_bare_item_dash_is_punctuation() {
    assert_eq!(
        highlighted("  - name: x"),
        [
            run(None, "  - "),
            run(Some(TokenKind::Key), "name"),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::Value), "x"),
        ]
    );
    assert_eq!(
        highlighted("- 1 # one"),
        [
            run(Some(TokenKind::Punctuation), "- "),
            run(Some(TokenKind::Literal), "1 "),
            run(Some(TokenKind::Comment), "# one"),
        ]
    );
}

#[test]
fn lines_matching_no_rule_stay_plain() {
    for line in ["url:http", "plain text", "-x", "  indented text", "{a: 1}"] {
        assert_eq!(highlighted(line), [run(None, line)], "{line:?}");
    }
    assert!(highlighted("").is_empty());
}

#[test]
fn highlighting_covers_every_character_of_tricky_lines() {
    for line in [
        "",
        "   ",
        "# only a comment",
        "name: héllo 🙂 # ünïcode",
        "- \"unterminated # still quoted",
        "key: 'it''s # quoted' # real",
        "key: value   ",
        "- ",
        "\u{3000}key: x",
        "a:\tb",
    ] {
        let text: String = highlighted(line).into_iter().map(|(_, text)| text).collect();
        assert_eq!(text, line);
    }
}

#[test]
fn comments_are_split_only_outside_yaml_quotes() {
    assert_eq!(
        split_yaml_comment("plain value # note"),
        ("plain value ", Some("# note"))
    );
    assert_eq!(
        split_yaml_comment("\"value # not a comment\""),
        ("\"value # not a comment\"", None)
    );
    assert_eq!(
        split_yaml_comment("'it''s # still quoted'"),
        ("'it''s # still quoted'", None)
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn keys_inside_an_inline_mapping_are_suggested_from_the_schema() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"clients":{"type":"object","additionalProperties":{"type":"object","properties":{"secret":{"type":"string"},"max_connections":{"type":"integer"}}}}}}"#,
    )
    .unwrap();
    let input = "clients:\n  acme: { sec";
    let options = completions(
        input,
        input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let option = options
        .iter()
        .find(|option| option.label == "secret")
        .expect("a key typed inside { } should be suggested");
    let (updated, _, _) = apply_completion(input, option);
    assert_eq!(updated, "clients:\n  acme: { secret: ");
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_error_inside_an_inline_mapping_points_at_its_key() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"clients":{"type":"object","additionalProperties":{"type":"object","properties":{"secret":{"type":"string"}}}}}}"#,
    )
    .unwrap();
    let source = "clients:\n  acme: { secret }\n";
    let diagnostics = validate_yaml(source, Some(&schema), None, true);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
    assert_eq!(diagnostics[0].column, Some(11));
}

/// A lone carriage return is a YAML line break (YAML 1.2, PyYAML): a file
/// written with them reads its data in every headless entry (`parse`,
/// `convert`, `validate`), wherever the break falls: after a comment, a
/// document marker, a value, inside a block scalar.
#[test]
fn a_lone_cr_breaks_lines_in_every_headless_entry() {
    use crate::Language;
    use serde_json::json;
    let cases = [
        ("# top\rname: edge\rn: 1\n", json!({"name": "edge", "n": 1})),
        ("\r# top\rname: x\n", json!({"name": "x"})),
        ("---\r# c\rname: x\n", json!({"name": "x"})),
        ("# a\n# b\rname: x\n", json!({"name": "x"})),
        ("x: 1\r# top\rname: x\n", json!({"x": 1, "name": "x"})),
        ("name: edge\r", json!({"name": "edge"})),
        ("l:\r  - a # c\r  - b\r", json!({"l": ["a", "b"]})),
        ("s: |\r  a\r  b\rn: 1\r", json!({"s": "a\nb\n", "n": 1})),
        ("# a\r\n# b\rname: x\r\n", json!({"name": "x"})),
    ];
    // A schema that refuses the data: validation sees it.
    let schema = build_schema(
        r#"{"properties":{"name":{"type":"integer"},"l":{"type":"integer"},"s":{"type":"integer"}}}"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for (text, expected) in cases {
        let parsed = Language::YAML.parse(text).ok();
        if parsed.as_ref() != Some(&expected) {
            failures.push(format!("parse {text:?}: {parsed:?}"));
        }
        let converted = Language::YAML
            .convert(text, Language::YAML)
            .ok()
            .and_then(|done| Language::YAML.parse(&done.text).ok());
        if converted.as_ref() != Some(&expected) {
            failures.push(format!("convert {text:?}: {converted:?}"));
        }
        let problems = validate_yaml(text, Some(&schema), None, true);
        if cfg!(feature = "schema-validation") && problems.is_empty() {
            failures.push(format!("validate {text:?}: no problem"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}


#[test]
fn custom_tags_are_valid_yaml_read_as_the_values_under_them() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"s":{"type":"string"},"l":{"type":"array"},"m":{"type":"object"}}}"#,
    )
    .unwrap();
    let cases = [
        ("!Sub", "s: !Sub 'arn:${AWS::Region}'\n"),
        ("!GetAtt", "l: !GetAtt [Bucket, Arn]\n"),
        ("!GetAtt dotted", "s: !GetAtt Bucket.Arn\n"),
        ("!include", "m: !include\n  k: v\n"),
        ("!secret", "s: !secret db_password\n"),
        ("!reference", "l: !reference [.setup, script]\n"),
        ("!Ref in a list", "l:\n  - !Ref A\n  - !Ref B\n"),
        ("verbatim tag", "s: !<tag:example.com,2026:x> v\n"),
        ("tag on a block map", "m: !Tag\n  a: 1\n  b: [1, 2]\n"),
        ("tag with a comment", "s: !Sub x # note\n"),
    ];
    let mut failures = Vec::new();
    for (name, text) in cases {
        let problems = validate_yaml(text, Some(&schema), None, true);
        if !problems.is_empty() {
            failures.push(format!("{name}: {:?}", problems.iter().map(|p| &p.message).collect::<Vec<_>>()));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn several_documents_say_so_in_plain_words_at_the_second() {
    for text in ["a: 1\n---\nb: 2\n", "---\na: 1\n---\nb: 2\n", "a: 1\n...\n---\nb: 2\n"] {
        let error = YamlBehavior.parse(text).expect_err(text);
        assert!(
            error.message.contains("more than one YAML document")
                && !error.message.contains("deserializ")
                && !error.message.contains("serde"),
            "{text:?}: {}",
            error.message
        );
        let second = text.rfind("---").unwrap();
        assert_eq!(error.line, Some(text[..second].matches('\n').count() + 1), "{text:?}");
    }
}

/// QA 22: a complex key's value line (`: y` under `- ? x`) was coloured as
/// the plain key going on, colon and all; its colon is punctuation.
#[test]
fn a_complex_keys_value_line_is_not_a_plain_scalar_going_on() {
    for source in ["list:\n  - ? x\n    : y\n", "? x\n: y\n", "a: b\n  : c\n"] {
        let constructs = YamlBehavior.multiline_constructs(source);
        let colon = source.rfind(':').unwrap();
        let covering: Vec<_> = constructs
            .iter()
            .filter(|construct| construct.range.contains(&colon))
            .collect();
        assert!(covering.is_empty(), "{source:?}: {covering:?}");
    }
}
