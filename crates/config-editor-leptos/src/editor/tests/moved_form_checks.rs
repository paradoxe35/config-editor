//! A moved key whose host form cannot be written where it is pasted makes
//! the save wait (never saved in another form with no problem
//! shown), and the save of a moved key compares only its own scalars with
//! where it was cut from, at any document size.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{key_line, line_offset, paste, pause, units};
use super::moved_forms_again::copy;
use crate::editor::work_count::{Work, counted};
use leptos::prelude::*;

/// The line of `name` cut from the view: its text.
fn cut_line(state: EditorState, name: &str) -> String {
    let before = state.doc().text.get_untracked();
    let line = key_line(&before, name);
    let (from, to) = (line_offset(&before, line), line_offset(&before, line + 1));
    let lines = before[from..to].to_string();
    let text = format!("{}{}", &before[..from], &before[to..]);
    let caret = units(&before[..from]);
    state.commit(Edit::new_units(
        before,
        (caret, caret + units(&lines)),
        text,
        caret,
    ));
    lines
}

/// A YAML block scalar pasted into a map the host writes inline (`b:
/// {…}`) cannot stay a block scalar there: the save waits, its host text
/// the one saved before.
#[test]
fn a_moved_value_whose_form_cannot_be_written_there_waits() {
    let host = "a:\n  sub: |\n    hello\n  mid: 1\nb: {other: 2}\n";
    let mut editor = TestEditor::new(Language::YAML, host);
    editor.edit_as = Some(Language::JSON);
    let (_owner, state) = editor.mount();
    let lines = cut_line(state, "sub");
    pause(state);
    let before = state.sync().host_text.get_value();
    assert_eq!(before, "a:\n  mid: 1\nb: {other: 2}\n");
    paste(state, &lines, "other", false);
    pause(state);
    let saved = state.sync().host_text.get_value();
    assert!(
        !state.sync().in_sync.get_untracked() && saved == before,
        "{saved}\n{:?}",
        state.sync().conversion_problems.get_untracked()
    );
}

/// The pause saving a key pasted back after a pause compares its scalars
/// alone with where it was cut from, and reads the texts as often, at any
/// size.
#[test]
fn a_moved_key_is_kept_with_the_same_work_at_any_size() {
    let work = |count: usize| {
        let fillers: String = (0..count)
            .map(|at| format!("  k{at} = var.v{at}\n"))
            .collect();
        let host = format!(
            "a {{\n  sub = aws_subnet.main.id\n  mid = 1\n{fillers}}}\nb {{\n  other = 2\n}}\n"
        );
        let mut editor = TestEditor::new(Language::HCL, &host);
        editor.edit_as = Some(Language::YAML);
        let (_owner, state) = editor.mount();
        let lines = cut_line(state, "sub");
        pause(state);
        paste(state, &lines, "other", false);
        let ((((), leaves), reads), parsed) = counted(Work::ParsedBytes, || {
            counted(Work::DocumentRead, || {
                counted(Work::MovedLeaf, || pause(state))
            })
        });
        let saved = state.sync().host_text.get_value();
        assert!(
            saved.contains("b {\n  sub = aws_subnet.main.id\n"),
            "{saved}"
        );
        (leaves, reads, parsed / host.len())
    };
    let small = work(20);
    assert_eq!(small, work(400));
    assert!(small.0 <= 2, "{} scalars compared", small.0);
}

/// A typing pause that moved nothing (a value typed) reads no item of the
/// view: its links tell the save nothing, its save does the work of a save
/// told nothing, at any size.
#[test]
fn a_typed_value_is_saved_without_reading_the_items() {
    use crate::editor::bridge::Bridge;
    use crate::editor::bridge::links::{Clipboard, Links, pieces};
    let work = |count: usize| {
        let fillers: String = (0..count)
            .map(|at| format!("  k{at} = var.v{at}\n"))
            .collect();
        let host = format!("a {{\n  sub = aws_subnet.main.id\n  mid = 1\n{fillers}}}\n");
        let bridge = Bridge::new(Language::HCL, Language::YAML, None).expect("two languages");
        let shown = bridge.to_edit(&host).expect("shown").text;
        let at = shown.find("mid: 1").expect("mid") + "mid: 1".len();
        let typed = format!("{}2{}", &shown[..at], &shown[at..]);
        let mut links = Links::fresh(&host, &shown, Some(bridge.reading()));
        let caret = units(&shown[..at]) + 1;
        let made = crate::editor::edit::Edit::input_units(
            shown.clone(),
            (caret - 1, caret - 1),
            typed.clone(),
            (caret, caret),
            "insertText".to_string(),
            0.0,
        );
        let mut clipboard = Clipboard::default();
        let pieces = pieces(&made, &clipboard);
        let ((), read) = counted(Work::DocumentRead, || {
            links.edited(Language::YAML.behavior(), &shown, &pieces, &mut clipboard);
            assert!(
                links
                    .told(Language::YAML.behavior(), &typed, &host, false)
                    .is_none()
            );
        });
        assert_eq!(read, 0, "the links read the items");
        let saved = bridge.to_host_linked(&typed, &host, &host, None, None);
        assert!(saved.expect("saved").contains("mid = 12"));
        read
    };
    assert_eq!(work(20), work(400));
}

/// The pause saving a copied key compares it alone with the items it may
/// be a copy of, at any size.
#[test]
fn a_copied_key_is_kept_with_the_same_work_at_any_size() {
    let work = |count: usize| {
        let fillers: String = (0..count).map(|at| format!("k{at} = {at}\n")).collect();
        let host = format!("[a]\nsub = 1979-05-27T07:32:00Z\nmid = 1\n{fillers}\n[b]\nother = 2\n");
        let mut editor = TestEditor::new(Language::TOML, &host);
        editor.edit_as = Some(Language::YAML);
        let (_owner, state) = editor.mount();
        let text = state.doc().text.get_untracked();
        let line = key_line(&text, "sub");
        let range = (line_offset(&text, line), line_offset(&text, line + 1));
        let lines = text[range.0..range.1].to_string();
        copy(state, range);
        paste(state, &lines, "other", false);
        let ((((), compared), leaves), reads) = counted(Work::DocumentRead, || {
            counted(Work::MovedLeaf, || {
                counted(Work::ItemComparison, || pause(state))
            })
        });
        let saved = state.sync().host_text.get_value();
        assert!(
            saved.ends_with("[b]\nsub = 1979-05-27T07:32:00Z\nother = 2\n"),
            "{saved}"
        );
        (compared, leaves, reads)
    };
    let small = work(20);
    assert_eq!(small, work(400));
    assert!(small.0 <= 2, "{} items compared", small.0);
}

/// A paste of many copied keys at once compares each with the keys
/// holding the same data only: work in proportion to them.
#[test]
fn many_copied_keys_are_kept_with_work_in_proportion() {
    let work = |count: usize| {
        let entries: String = (0..count)
            .map(|at| format!("k{at} = 1979-05-27T07:32:00Z\n"))
            .collect();
        let host = format!("[a]\n{entries}\n[b]\nother = 2\n");
        let mut editor = TestEditor::new(Language::TOML, &host);
        editor.edit_as = Some(Language::YAML);
        let (_owner, state) = editor.mount();
        let text = state.doc().text.get_untracked();
        let (from, to) = (
            line_offset(&text, key_line(&text, "k0")),
            line_offset(&text, key_line(&text, "b")),
        );
        let lines = text[from..to].to_string();
        copy(state, (from, to));
        paste(state, &lines, "other", false);
        let ((), compared) = counted(Work::ItemComparison, || pause(state));
        let saved = state.sync().host_text.get_value();
        assert_eq!(saved, format!("[a]\n{entries}\n[b]\n{entries}other = 2\n"));
        compared
    };
    for count in [20, 400] {
        let compared = work(count);
        assert!(
            compared <= count + 2,
            "{compared} comparisons for {count} keys"
        );
    }
}
