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

#[test]
fn keyboard_accepts_completions_only_after_explicit_engagement() {
    // Auto-triggered list, highlight untouched: Tab must NOT accept (it
    // indents). Enter accepts whenever a list is open, so it does not use
    // this gate.
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

#[test]
fn replacing_a_selection_respects_utf16_cursor_offsets() {
    let (updated, cursor) = replace_selection("ab🙂xy", 4, 5, "Z");
    assert_eq!(updated, "ab🙂Zy");
    assert_eq!(cursor, 5);
}

#[test]
fn indentation_unit_matches_the_document_style() {
    use super::commands::{DEFAULT_INDENT_WIDTH, INDENT_SCAN_LINE_LIMIT, MAX_INDENT_WIDTH};
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
        indent_lines(source, 0, source.encode_utf16().count(), false).unwrap();
    assert_eq!(indented, "  one:\n  two:\n");
    assert_eq!(start, 0);
    assert_eq!(end, indented.encode_utf16().count());

    let (outdented, _, _) = indent_lines(&indented, start, end, true).unwrap();
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
