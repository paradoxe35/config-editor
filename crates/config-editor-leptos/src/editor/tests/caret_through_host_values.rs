//! A host value replacing the text keeps the caret on the text it was on,
//! and the caret's line and path follow it there (the active line, the
//! breadcrumb, `currentPath`), as for any other change of the text.

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::sync::take_host_value;
use crate::editor::test_state::TestEditor;

/// An editor on `text` (typed over the host's own) with the caret at UTF-16
/// `caret`.
fn editor_at(text: &str, caret: usize) -> (Owner, EditorState) {
    let (owner, state) = TestEditor::new(Language::YAML, "seed: 0\n").mount();
    let before = state.doc().text.get_untracked();
    let end = before.encode_utf16().count();
    assert!(state.commit(Edit::new(before, (0, end), text.to_string(), caret)));
    assert_eq!(state.doc().caret.get_untracked(), caret);
    (owner, state)
}

#[test]
fn a_host_value_emptying_the_text_puts_the_caret_on_its_only_line() {
    let text = "👍🏽👨‍👩‍👧\n";
    let (_owner, state) = editor_at(text, text.encode_utf16().count());
    assert_eq!(state.doc().current_line.get_untracked(), 2);
    take_host_value(state, String::new());
    assert_eq!(state.doc().caret.get_untracked(), 0);
    assert_eq!(state.doc().current_line.get_untracked(), 1);
    assert!(state.doc().caret_path.get_untracked().is_root());
}

#[test]
fn a_host_value_above_the_caret_moves_its_line_and_path_with_it() {
    let (_owner, state) = editor_at("a: 1\nb: 2\n", 6);
    assert_eq!(state.doc().caret_path.get_untracked().to_pointer(), "/b");
    take_host_value(state, "x: 0\na: 1\nb: 2\n".to_string());
    assert_eq!(state.doc().caret.get_untracked(), 11);
    assert_eq!(state.doc().current_line.get_untracked(), 3);
    assert_eq!(state.doc().caret_path.get_untracked().to_pointer(), "/b");
}
