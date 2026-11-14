//! The editor keeps the edits made to an Edit-as view between saves, and
//! each typing pause's save reads which item is which from them: an item
//! cut and pasted after a pause keeps its `xsi:type` (QA `E32-2`), and the
//! first of two items with the same data deleted takes its own away (QA
//! `E32-3`), as typed in the editor.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use leptos::prelude::*;

const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

fn mount(text: &str) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::XML, text);
    editor.edit_as = Some(Language::YAML);
    editor.mount()
}

/// The view's text with `removed` (whole lines) cut, the caret left where
/// they were; or with `inserted` put at the start of the line holding
/// `before`, the caret after it.
fn cut(state: EditorState, removed: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(removed).expect("the lines");
    let text = before.replacen(removed, "", 1);
    let caret = before[..at].encode_utf16().count();
    state.commit(Edit::new(before, (caret, caret), text, caret));
}

fn paste(state: EditorState, inserted: &str, before_line: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(before_line).expect("the line");
    let at = before[..at].rfind('\n').map_or(0, |line| line + 1);
    let text = format!("{}{inserted}{}", &before[..at], &before[at..]);
    let caret = before[..at].encode_utf16().count() + inserted.encode_utf16().count();
    let start = before[..at].encode_utf16().count();
    state.commit(Edit::new(before, (start, start), text, caret));
}

fn pause(state: EditorState) {
    convert_now(state, state.doc().text.get_untracked(), Language::YAML);
}

#[test]
fn an_item_cut_and_pasted_after_a_pause_keeps_its_markup() {
    let host = format!(
        "<nlog {XSI}>\n  <targets>\n    <target xsi:type=\"File\" name=\"logfile\" />\n    <target xsi:type=\"Console\" name=\"console\" />\n    <target xsi:type=\"Null\" name=\"blackhole\" />\n  </targets>\n</nlog>\n"
    );
    let (_owner, state) = mount(&host);
    let line = state
        .doc()
        .text
        .get_untracked()
        .lines()
        .find(|line| line.contains("name: console"))
        .expect("the console line")
        .to_string()
        + "\n";
    cut(state, &line);
    pause(state);
    paste(state, &line, "name: logfile");
    pause(state);
    let saved = state.sync().host_text.get_value();
    let console = saved.find("name=\"console\"").expect("saved");
    let logfile = saved.find("name=\"logfile\"").expect("saved");
    assert!(console < logfile, "{saved}");
    assert!(
        saved.contains("<target xsi:type=\"Console\" name=\"console\" />"),
        "{saved}"
    );
}

#[test]
fn deleting_the_first_of_two_alike_items_takes_its_markup_away() {
    let host = format!(
        "<shapes {XSI}>\n  <shape xsi:type=\"Circle\">\n    <size>1</size>\n  </shape>\n  <shape xsi:type=\"Square\">\n    <size>1</size>\n  </shape>\n</shapes>\n"
    );
    let (_owner, state) = mount(&host);
    let line = state
        .doc()
        .text
        .get_untracked()
        .lines()
        .find(|line| line.contains("size: 1"))
        .expect("an item line")
        .to_string()
        + "\n";
    cut(state, &line);
    pause(state);
    let saved = state.sync().host_text.get_value();
    assert!(
        saved.contains("Square") && !saved.contains("Circle"),
        "{saved}"
    );
}
