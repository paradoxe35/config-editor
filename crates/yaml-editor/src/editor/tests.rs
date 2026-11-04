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
    assert_eq!(ending.deleted_after, true);
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
        "auth:\n  enabled: false\n  token: \n  mode: \"safe\"\n"
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
    let input = "clients:\n  paradoxe:\n    ";
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
                    "prefixItems":[{"enum":["fast","safe"]}]
                }
            }
        }"#,
    )
    .unwrap();

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
