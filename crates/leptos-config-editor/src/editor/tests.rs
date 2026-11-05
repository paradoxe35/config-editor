use super::*;

#[test]
fn theme_names_and_layout_families_are_stable() {
    let themes = [
        (EditorTheme::VsCodeDark, "vscode-dark", "vscode-dark"),
        (EditorTheme::VsCodeLight, "vscode-light", "vscode-light"),
        (EditorTheme::Obsidian, "obsidian", "obsidian"),
        (
            EditorTheme::ObsidianLight,
            "obsidian-light",
            "obsidian-light",
        ),
        (EditorTheme::Dracula, "dracula", "obsidian"),
        (EditorTheme::Nord, "nord", "obsidian"),
        (EditorTheme::AtomOneDark, "atom-one-dark", "obsidian"),
        (EditorTheme::GitHubLight, "github-light", "vscode-light"),
        (EditorTheme::GitHubDark, "github-dark", "obsidian"),
        (
            EditorTheme::SolarizedLight,
            "solarized-light",
            "vscode-light",
        ),
        (EditorTheme::ViraInspired, "vira-inspired", "obsidian"),
    ];

    for (theme, name, family) in themes {
        assert_eq!(theme.as_str(), name);
        assert_eq!(theme.css_family(), family);
    }
}

fn test_schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{"public_address":{"type":"string","description":"Address"},"websocket":{"type":"object","properties":{"challenge":{"enum":["tls","http","dns"]}}}},"additionalProperties":false}"#,
    )
    .unwrap()
}

/// Schema exercising nullable objects, `$ref` branches, `additionalProperties`
/// with dynamic keys, enums, and numeric-as-string examples. It lives in
/// `tests/fixtures/` (not the workspace-root demo schema) so crate tests never
/// reach outside the crate.
#[cfg(feature = "completion")]
fn fixture_schema() -> SchemaBundle {
    build_schema(include_str!("../../tests/fixtures/schema.json")).unwrap()
}

#[test]
fn line_diff_marks_inserted_lines_and_preserves_unchanged_lines() {
    let diff = diff_lines("first:\nsecond:\nthird:", "first:\nadded:\nsecond:\nthird:");
    assert_eq!(
        diff.current,
        vec![
            DiffKind::Unchanged,
            DiffKind::Added,
            DiffKind::Unchanged,
            DiffKind::Unchanged,
        ]
    );
    assert_eq!(diff.deleted_before, vec![false; 4]);
}

#[test]
fn line_diff_keeps_existing_baseline_content_unchanged_and_marks_only_new_content() {
    let baseline = "name: app\nlisten: 7835\n";
    let current = "name: app\nlisten: 7835\ninsecure_plaintext: false\n";
    let diff = diff_lines(baseline, current);
    assert_eq!(
        diff.current,
        vec![
            DiffKind::Unchanged,
            DiffKind::Unchanged,
            DiffKind::Added,
            DiffKind::Unchanged,
        ]
    );
    assert!(diff.deleted_before.iter().all(|deleted| !deleted));
    assert!(!diff.deleted_after);
}

#[test]
fn line_diff_marks_replacements_as_modified_and_deletions_in_the_gutter() {
    let diff = diff_lines("first:\nold:\nremove:\nlast:", "first:\nnew:\nlast:");
    assert_eq!(
        diff.current,
        vec![DiffKind::Unchanged, DiffKind::Modified, DiffKind::Unchanged,]
    );
    assert_eq!(diff.deleted_before, vec![false, false, true]);
}

#[test]
fn line_diff_marks_inserted_blank_lines_without_shifting_following_lines() {
    let baseline = "first:\nsecond:\nthird:\n";
    let current = "first:\n\n\nsecond:\nthird:\n";
    let diff = diff_lines(baseline, current);
    assert_eq!(
        diff.current,
        vec![
            DiffKind::Unchanged,
            DiffKind::Added,
            DiffKind::Added,
            DiffKind::Unchanged,
            DiffKind::Unchanged,
            DiffKind::Unchanged,
        ]
    );
    assert!(diff.deleted_before.iter().all(|deleted| !deleted));
}

#[test]
fn line_diff_keeps_following_lines_unchanged_after_insertions_at_eof() {
    let baseline = "first:\nsecond:\n";
    let current = "first:\nsecond:\n\n\n";
    let diff = diff_lines(baseline, current);
    assert_eq!(
        diff.current,
        vec![
            DiffKind::Unchanged,
            DiffKind::Unchanged,
            DiffKind::Added,
            DiffKind::Added,
            DiffKind::Unchanged,
        ]
    );
}

#[test]
fn line_diff_tracks_deletions_before_first_and_after_last_lines() {
    let beginning = diff_lines("remove:\nkeep:\n", "keep:\n");
    assert_eq!(
        beginning.current,
        vec![DiffKind::Unchanged, DiffKind::Unchanged]
    );
    assert_eq!(beginning.deleted_before, vec![true, false]);

    let ending = diff_lines("keep:\nremove:", "keep:");
    assert_eq!(ending.current, vec![DiffKind::Unchanged]);
    assert_eq!(ending.deleted_before, vec![false]);
    assert!(ending.deleted_after);
}

#[test]
fn line_diff_clears_after_baseline_is_accepted() {
    let current = "first:\nnew:\nlast:";
    let diff = diff_lines(current, current);
    assert!(
        diff.current
            .iter()
            .all(|change| *change == DiffKind::Unchanged)
    );
    assert!(diff.deleted_before.iter().all(|deleted| !deleted));
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
    let (updated, selection_start, selection_end) = paired_edit("ab", 1, 1, '[').unwrap();
    assert_eq!(updated, "a[]b");
    assert_eq!((selection_start, selection_end), (2, 2));
}

#[test]
fn opening_bracket_does_not_duplicate_a_closing_bracket_ahead() {
    let (updated, selection_start, selection_end) = paired_edit(")", 0, 0, '(').unwrap();
    assert_eq!(updated, "()");
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
fn closing_pair_typing_moves_over_the_existing_character() {
    assert_eq!(skip_closing_pair("text)", 4, ')'), Some(5));
    assert_eq!(skip_closing_pair("text]", 4, ')'), None);
}

#[test]
fn backspace_removes_an_empty_pair() {
    assert_eq!(
        delete_empty_pair("value: []", 8),
        Some(("value: ".to_string(), 7))
    );
    assert_eq!(delete_empty_pair("value: [x]", 8), None);
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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
    assert_eq!(option.type_label, "enum");
}

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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
    let (updated, cursor) = apply_completion("", option);
    assert_eq!(updated, option.insert);
    assert_eq!(
        cursor,
        "auth:\n  enabled: false\n  token: ".encode_utf16().count()
    );
}

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
#[test]
fn examples_and_example_schema_values_are_suggested_in_value_positions() {
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

    let region_input = "region: eu";
    let region_options = completions(
        region_input,
        region_input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    let region = region_options
        .iter()
        .find(|option| option.label == "eu-west-1")
        .unwrap();
    assert!(region.is_value);
    assert_eq!(region.insert, "eu-west-1");

    let legacy_input = "legacy: ol";
    let legacy_options = completions(
        legacy_input,
        legacy_input.encode_utf16().count(),
        false,
        Some(&schema),
        true,
    );
    assert!(
        legacy_options
            .iter()
            .any(|option| option.label == "old.example")
    );
}

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
#[test]
fn match_ranges_cover_the_typed_characters_for_every_tier() {
    use super::completion::match_ranges;
    assert!(match_ranges("max_connections", "").is_empty());
    assert_eq!(match_ranges("listen", "lis"), vec![(0, 3)]);
    assert_eq!(match_ranges("Listen", "lis"), vec![(0, 3)]);
    assert_eq!(match_ranges("max_connections", "mc"), vec![(0, 1), (4, 5)]);
    assert_eq!(match_ranges("insecure_plaintext", "sec"), vec![(2, 5)]);
    assert_eq!(match_ranges("listen", "ltn"), vec![(0, 1), (3, 4), (5, 6)]);
    assert!(match_ranges("listen", "xyz").is_empty());
    // Ranges index characters, not bytes.
    assert_eq!(match_ranges("héllo", "é"), vec![(1, 2)]);
}

#[cfg(feature = "completion")]
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
    let (updated, cursor) = apply_completion(input, option);
    assert_eq!(updated, "\"display:name\": ");
    assert_eq!(cursor, updated.encode_utf16().count());
}

#[cfg(feature = "completion")]
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
    assert_eq!(
        option.insert,
        "auth:\n        enabled: false\n        token: \n"
    );
}

#[test]
fn keyboard_accepts_completions_only_after_explicit_engagement() {
    // Auto-triggered list, highlight untouched: Enter/Tab must NOT accept.
    // This pins the P0-2 fix: typing `lis` then Enter inserts a newline
    // instead of hijacking the keystroke for `listen: `.
    assert!(!should_accept_with_keyboard(false, false));
    // Ctrl+Space (or an accept-chained value list): Enter accepts.
    assert!(should_accept_with_keyboard(true, false));
    // Arrow keys or hover moved the highlight: Enter/Tab accept.
    assert!(should_accept_with_keyboard(false, true));
    assert!(should_accept_with_keyboard(true, true));
}

#[test]
fn auto_trigger_only_fires_for_genuine_insertions() {
    assert!(should_auto_trigger("insertText", "", "n"));
    assert!(should_auto_trigger("insertText", "na", "nam"));
    assert!(should_auto_trigger("insertCompositionText", "", "e"));
    assert!(should_auto_trigger("insertText", "old-key", "s"));
    assert!(should_auto_trigger(
        "insertCompositionText",
        "old-key",
        "新"
    ));

    assert!(!should_auto_trigger(
        "insertLineBreak",
        "a:\n  ",
        "a:\n  \n  "
    ));
    assert!(!should_auto_trigger("insertParagraph", "a", "a\n"));
    assert!(should_auto_trigger("insertText", "abc", "ab"));
    assert!(!should_auto_trigger("", "abc", "ab"));
    assert!(!should_auto_trigger("insertFromPaste", "", "pasted"));
    assert!(!should_auto_trigger("insertFromDrop", "", "dropped"));
    assert!(!should_auto_trigger("deleteContentBackward", "ab", "a"));
    assert!(!should_auto_trigger("deleteContentForward", "ab", "a"));
    assert!(!should_auto_trigger("historyUndo", "a", "ab"));
    assert!(!should_auto_trigger("historyRedo", "a", "ab"));
    assert!(!should_auto_trigger("insertText", "a", "a\n"));
}

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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
    let (updated, cursor) = apply_completion(input, option);
    assert_eq!(updated, "web:\n  http: \"0.0.0.0:7835\"");
    assert_eq!(cursor, updated.encode_utf16().count());
}

#[test]
fn replacing_a_selection_respects_utf16_cursor_offsets() {
    let (updated, cursor) = replace_selection("ab🙂xy", 4, 5, "Z");
    assert_eq!(updated, "ab🙂Zy");
    assert_eq!(cursor, 5);
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

    let empty_item = "services:\n  - ";
    assert_eq!(newline_with_indent(empty_item, empty_item.len()), "\n  - ");

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

#[test]
fn indentation_unit_matches_the_document_style() {
    use super::commands::{
        DEFAULT_INDENT_WIDTH, INDENT_SCAN_LINE_LIMIT, MAX_INDENT_WIDTH,
    };
    // Pins the scan bounds to their documented rationale: a screenful scan,
    // eight-space alignment filter, two-space fallback.
    assert_eq!(INDENT_SCAN_LINE_LIMIT, 400);
    assert_eq!(MAX_INDENT_WIDTH, 8);
    assert_eq!(DEFAULT_INDENT_WIDTH, 2);
    assert_eq!(indent_unit(""), "  ");
    assert_eq!(indent_unit("one:\n  two: 1\n"), "  ");
    assert_eq!(indent_unit("one:\n    two: 1\n"), "    ");
    assert_eq!(indent_unit("one:\n\ttwo: 1\n"), "\t");
}

#[test]
fn tab_indents_selected_lines_and_shift_tab_outdents_them() {
    let source = "one:\ntwo:\n";
    let (indented, start, end) =
        indent_yaml_lines(source, 0, source.encode_utf16().count(), false).unwrap();
    assert_eq!(indented, "  one:\n  two:\n");
    assert_eq!(start, 0);
    assert_eq!(end, indented.encode_utf16().count());

    let (outdented, _, _) = indent_yaml_lines(&indented, start, end, true).unwrap();
    assert_eq!(outdented, source);
}

#[test]
fn offset_at_maps_line_cells_to_utf16_offsets_and_clamps() {
    // "one: 1\n🙂: 2\nthree: 3\n": line 2 starts after 7+1 units, and the
    // emoji occupies two UTF-16 units, so column math must count UTF-16.
    let source = "one: 1\n\u{1F642}: 2\nthree: 3\n";
    assert_eq!(offset_at(source, 1, 1), 0);
    assert_eq!(offset_at(source, 1, 4), 3);
    assert_eq!(offset_at(source, 2, 1), 7);
    assert_eq!(offset_at(source, 2, 2), 9);
    assert_eq!(offset_at(source, 3, 6), 18);
    // Past the end of a line or document clamps instead of panicking.
    assert_eq!(offset_at(source, 2, 99), 12);
    assert_eq!(offset_at(source, 99, 1), source.encode_utf16().count());
    assert_eq!(offset_at("", 1, 1), 0);
}

#[test]
fn f8_cycles_through_diagnostic_offsets_and_wraps_at_the_ends() {
    let offsets = [5, 12, 20];
    assert_eq!(cycle_offset(&[], 0, false), None);
    assert_eq!(cycle_offset(&offsets, 0, false), Some(0));
    assert_eq!(cycle_offset(&offsets, 5, false), Some(1));
    assert_eq!(cycle_offset(&offsets, 13, false), Some(2));
    assert_eq!(cycle_offset(&offsets, 20, false), Some(0));
    assert_eq!(cycle_offset(&offsets, 99, false), Some(0));
    assert_eq!(cycle_offset(&offsets, 99, true), Some(2));
    assert_eq!(cycle_offset(&offsets, 20, true), Some(1));
    assert_eq!(cycle_offset(&offsets, 5, true), Some(2));
    assert_eq!(cycle_offset(&offsets, 0, true), Some(2));
    assert_eq!(cycle_offset(&[7], 0, false), Some(0));
    assert_eq!(cycle_offset(&[7], 7, false), Some(0));
}

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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
    let (updated, cursor) = apply_completion(input, &option);
    assert_eq!(updated, "web:\n  http: \"0.0.0.0:7835\"");
    assert_eq!(cursor, updated.encode_utf16().count());
}

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
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
    let (updated, _) = apply_completion(input, &port);
    assert_eq!(updated, "listen: 7835");
}

#[cfg(feature = "completion")]
#[test]
fn enum_words_are_inserted_as_plain_scalars_and_ambiguous_strings_stay_quoted() {
    use super::completion::yaml_value;
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

#[cfg(feature = "completion")]
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

#[cfg(feature = "completion")]
#[test]
fn value_for_an_expanded_required_key_stays_on_its_line() {
    let schema = fixture_schema();
    // This is exactly what accepting `metrics` inserts: a trailing newline follows
    // the empty value of its required `listen` key.
    let input = "metrics:\n  listen: \n";
    let cursor = "metrics:\n  listen: ".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "0.0.0.0:7835")
        .expect("metrics.listen should offer Bind examples");
    let (updated, cursor_after) = apply_completion(input, &option);
    assert_eq!(updated, "metrics:\n  listen: \"0.0.0.0:7835\"\n");
    assert_eq!(
        cursor_after,
        "metrics:\n  listen: \"0.0.0.0:7835\""
            .encode_utf16()
            .count()
    );
}

#[test]
fn auto_trigger_still_fires_when_the_document_ends_with_a_newline() {
    assert!(should_auto_trigger("insertText", "a: 1\n", "a: 1\nl"));
    assert!(should_auto_trigger(
        "insertText",
        "a: 1\nb: 2\n",
        "la: 1\nb: 2\n"
    ));
    assert!(!should_auto_trigger("insertLineBreak", "a: 1", "a: 1\n"));
    assert!(!should_auto_trigger("insertText", "a: 1", "a: 1\n"));
}

#[cfg(feature = "completion")]
#[test]
fn typing_inside_a_value_uses_the_text_before_the_caret_and_replaces_the_token() {
    let schema = fixture_schema();
    let input = "update:\n  remote: ftrue";
    let cursor = "update:\n  remote: f".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "false")
        .expect("typing before existing text should suggest the matching value");
    let (updated, cursor_after) = apply_completion(input, &option);
    assert_eq!(updated, "update:\n  remote: false");
    assert_eq!(cursor_after, updated.encode_utf16().count());
}

#[cfg(feature = "completion")]
#[test]
fn accepting_a_value_keeps_an_inline_comment() {
    let schema = fixture_schema();
    let input = "update:\n  remote: tru # keep me";
    let cursor = "update:\n  remote: tru".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "true")
        .expect("true should match the partial value");
    let (updated, _) = apply_completion(input, &option);
    assert_eq!(updated, "update:\n  remote: true # keep me");
}

#[cfg(feature = "completion")]
#[test]
fn completion_replace_range_survives_multibyte_characters_before_the_caret() {
    let schema = build_schema(
        r#"{"type":"object","properties":{"café":{"type":"string"},"listen":{"type":"integer"}}}"#,
    )
    .unwrap();
    // `é` is one UTF-16 unit but two UTF-8 bytes, so byte and UTF-16 offsets
    // diverge before the caret. A byte-based replace end would overshoot and
    // eat the trailing value when the completion is accepted.
    let input = "café: 1\nlis: 2";
    let cursor = "café: 1\nlis".encode_utf16().count();
    let option = completions(input, cursor, false, Some(&schema), true)
        .into_iter()
        .find(|option| option.label == "listen")
        .expect("partial key should complete to the schema property");
    let (updated, _) = apply_completion(input, &option);
    assert_eq!(updated, "café: 1\nlisten: 2");
}
