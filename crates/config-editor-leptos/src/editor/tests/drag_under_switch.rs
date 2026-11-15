//! A drag carries the text it began in: a switch waiting for the document
//! to convert waits for the drag to end (it would rewrite the text under
//! the pointer, and the drop would copy the old text into the new one),
//! and a drop whose document was rewritten anyway is refused.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;

#[test]
fn a_waiting_switch_waits_for_a_drag_and_is_made_when_it_ends() {
    let text = "a: false\n😀";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    switch_to(state, Language::JSON);
    assert!(state.sync().pending_switch.get_untracked().is_some());
    state.drag_began();
    // The document converts now; the pause comes while the drag goes on.
    let fixed = "a: false\n";
    state.commit(Edit::new(text.to_string(), (9, 11), fixed.into(), 9));
    state.check_text_now();
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
    assert_eq!(state.doc().text.get_untracked(), fixed);
    state.drag_ended();
    assert_eq!(state.sync().edit_language.get_untracked(), Language::JSON);
}

#[test]
fn a_drop_into_a_document_rewritten_under_its_drag_is_refused() {
    let text = "a: 1\nb: 2\n";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    state.drag_began();
    assert!(!state.drop_refused_early());
    // Rewritten while the drag goes on (a host value).
    let rewritten = "c: 3\n";
    state.commit(Edit::new(text.to_string(), (0, 10), rewritten.into(), 0));
    assert!(state.rewritten_under_drag());
    assert!(state.drop_refused_early());
    state.drag_ended();
    assert!(!state.drop_refused_early());
}
