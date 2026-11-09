use super::EditorTheme;
use super::commands::{cycle_offset, indent_lines, indent_unit, offset_at};
use super::completion::should_auto_trigger;
use super::keyboard::{replace_selection, skip_closing_pair};

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

#[test]
fn closing_pair_typing_moves_over_the_existing_character() {
    assert_eq!(skip_closing_pair("text)", 4, ')'), Some(5));
    assert_eq!(skip_closing_pair("text]", 4, ')'), None);
    // An escaped quote is text, not the string's end.
    assert_eq!(skip_closing_pair("\"a\\\"", 3, '"'), None);
    assert_eq!(skip_closing_pair("\"a\\\\\"", 4, '"'), Some(5));
}

#[cfg(feature = "schema-validation")]
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

#[test]
fn replacing_a_selection_respects_utf16_cursor_offsets() {
    let (updated, cursor) = replace_selection("ab🙂xy", 4, 5, "Z");
    assert_eq!(updated, "ab🙂Zy");
    assert_eq!(cursor, 5);
}

#[test]
fn indentation_unit_matches_the_document_style() {
    use super::commands::{DEFAULT_INDENT_WIDTH, INDENT_SCAN_LINE_LIMIT, MAX_INDENT_WIDTH};
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
        indent_lines(source, 0, source.encode_utf16().count(), false, "  ").unwrap();
    assert_eq!(indented, "  one:\n  two:\n");
    assert_eq!(start, 0);
    assert_eq!(end, indented.encode_utf16().count());

    let (outdented, _, _) = indent_lines(&indented, start, end, true, "  ").unwrap();
    assert_eq!(outdented, source);
}

#[test]
fn offset_at_maps_line_cells_to_utf16_offsets_and_clamps() {
    // The emoji is two UTF-16 units.
    let source = "one: 1\n\u{1F642}: 2\nthree: 3\n";
    assert_eq!(offset_at(source, 1, 1), 0);
    assert_eq!(offset_at(source, 1, 4), 3);
    assert_eq!(offset_at(source, 2, 1), 7);
    assert_eq!(offset_at(source, 2, 2), 9);
    assert_eq!(offset_at(source, 3, 6), 18);
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

#[test]
fn tab_moves_a_selection_start_inside_the_indentation_with_its_text() {
    let source = "{\n    \"a\": 1,\n    \"b\": 2\n}\n";
    let start = source.find("\"a\"").unwrap();
    let end = source.find("2").unwrap() + 1;
    let (indented, new_start, new_end) = indent_lines(source, start, end, false, "  ").unwrap();
    assert_eq!(&indented[new_start..new_end], "\"a\": 1,\n      \"b\": 2");
    // From column 0 the selection keeps covering the indent.
    let (indented, new_start, _) = indent_lines(source, 2, end, false, "  ").unwrap();
    assert_eq!(new_start, 2);
    assert!(indented.starts_with("{\n      \"a\""));
}

#[test]
fn a_selection_starting_inside_the_indentation_keeps_its_column_through_tab_and_back() {
    let source = "tcp:\n  nodelay: true\n  keepalive_secs: 30\n";
    let start = 5 + 1;
    let end = source.find("30").unwrap() + 1;
    let (indented, new_start, new_end) = indent_lines(source, start, end, false, "  ").unwrap();
    assert_eq!(new_start, 5 + 3, "{indented:?}");
    let (outdented, back_start, _) =
        indent_lines(&indented, new_start, new_end, true, "  ").unwrap();
    assert_eq!(outdented, source);
    assert_eq!(back_start, start);
}
