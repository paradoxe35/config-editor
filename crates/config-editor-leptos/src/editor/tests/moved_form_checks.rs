//! A moved key whose host form cannot be written where it is pasted makes
//! the save wait (QA `E36-1`: never saved in another form with no problem
//! shown), and the save of a moved key compares only its own scalars with
//! where it was cut from, at any document size.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{key_line, line_offset, paste, pause, units};
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
    state.commit(Edit::new(
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
/// view: its save does the work of a save told nothing, at any size, and
/// the trail goes on from where it began.
#[test]
fn a_typed_value_is_saved_without_reading_the_items() {
    use crate::editor::bridge::{Bridge, item_trail::Trail};
    let work = |count: usize| {
        let fillers: String = (0..count)
            .map(|at| format!("  k{at} = var.v{at}\n"))
            .collect();
        let host = format!("a {{\n  sub = aws_subnet.main.id\n  mid = 1\n{fillers}}}\n");
        let bridge = Bridge::new(Language::HCL, Language::YAML, None).expect("two languages");
        let shown = bridge.to_edit(&host).expect("shown").text;
        let at = shown.find("mid: 1").expect("mid") + "mid: 1".len();
        let typed = format!("{}2{}", &shown[..at], &shown[at..]);
        let mut trail = Trail::starting(&shown);
        trail.note(&shown, &typed, units(&shown[..at]) + 1);
        let reads = |trail: Option<(&Trail, &str)>| {
            counted(Work::DocumentRead, || {
                let _ = bridge.to_host_traced(&typed, &host, &host, trail);
            })
            .1
        };
        let (told, untold) = (reads(Some((&trail, &host))), reads(None));
        let (saved, next) = bridge.to_host_traced(&typed, &host, &host, Some((&trail, &host)));
        assert!(saved.expect("saved").contains("mid = 12"));
        assert!(next.begins_on(&typed) && next.start() == Some(shown.as_str()));
        (told, untold)
    };
    let (told, untold) = work(20);
    assert_eq!(told, untold);
    assert_eq!((told, untold), work(400));
}

/// The pause saving a copied key compares it alone with the items it may
/// be a copy of, at any size (QA `E37-5`).
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
        let lines = text[line_offset(&text, line)..line_offset(&text, line + 1)].to_string();
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
/// holding the same data only: work in proportion to them (QA `E37-5`,
/// beside QA `SP37-1`'s large edits).
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
