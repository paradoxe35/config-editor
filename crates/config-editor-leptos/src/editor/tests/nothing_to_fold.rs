//! QA 32, A32-2: a fold key that changes nothing says so ("Nothing to
//! fold", "Nothing to unfold") instead of staying silent; one that folds or
//! opens says what it did, as before.

use leptos::prelude::*;

use crate::EditorStrings;
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

fn said(state: EditorState) -> String {
    state
        .a11y()
        .announcement
        .get_untracked()
        .trim_end_matches('\u{a0}')
        .to_string()
}

fn fold(strings: &EditorStrings) -> &String {
    &strings.nothing_to_fold
}

fn unfold(strings: &EditorStrings) -> &String {
    &strings.nothing_to_unfold
}

#[test]
fn a_fold_key_with_nothing_to_do_says_so() {
    let (_owner, state) = TestEditor::new(Language::YAML, "a:\n  b: 1\nc: 2\n").mount();
    // Line 3 (`c: 2`) starts no range and sits in none.
    state.fold_key_command(fold, || {
        state.fold_line(2);
    });
    assert_eq!(said(state), "Nothing to fold");
    state.fold_key_command(unfold, || {
        state.unfold_line(0);
    });
    assert_eq!(said(state), "Nothing to unfold");

    // Something to do: what it did, as before.
    state.fold_key_command(fold, || {
        state.fold_line(0);
    });
    assert_eq!(said(state), "Folded lines 1–2");
    // The same range a second time: already folded.
    state.fold_key_command(fold, || {
        state.fold_line(0);
    });
    assert_eq!(said(state), "Nothing to fold");
    state.fold_key_command(fold, || state.fold_all());
    assert_eq!(said(state), "Nothing to fold");
    state.fold_key_command(unfold, || state.unfold_all());
    assert_eq!(said(state), "Unfolded lines 1–2");
    state.fold_key_command(unfold, || state.unfold_all());
    assert_eq!(said(state), "Nothing to unfold");
}
