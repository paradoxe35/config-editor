//! A request for suggestions that filtered down to no options shows no
//! list, so Escape (and every key that asks whether a popup is open) goes
//! on to the next thing instead of being swallowed by it.

use leptos::prelude::*;

use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

#[test]
fn a_request_without_options_is_not_a_popup_for_escape() {
    let (_owner, state) = TestEditor::new(Language::YAML, "name: xyz\n").mount();
    state.set_list_mode(crate::editor::completion::ListMode::TYPED);
    assert!(
        state.completion().suggestions.with_untracked(Vec::is_empty),
        "the sample must leave the list empty"
    );
    assert!(!state.suggestions_showing());
    assert!(
        !state.escape_suggestions(),
        "nothing showed, nothing closed"
    );
    assert!(!state.list_mode().is_open(), "the stale request is dropped");
}
