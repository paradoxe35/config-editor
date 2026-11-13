//! The problems shown are capped per kind: thousands of secret placeholder
//! warnings (a document pasted with tokens out of their fields) show and
//! underline as many as the schema's problems, with the note that more
//! follow, so every key does not move thousands of underlines.

use leptos::prelude::*;

use crate::editor::diagnostics::{MAX_DIAGNOSTICS, MORE_PROBLEMS_CODE, Severity};
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

#[test]
fn thousands_of_placeholder_warnings_are_capped_with_a_note() {
    let host = "secret: «secret 1»\n";
    let (_owner, state) = TestEditor::new(Language::YAML, host).mount();
    // The host's token written in 500 other fields: each copy is flagged.
    let copies: String = (0..500)
        .map(|index| format!("n{index}: «secret 1»\n"))
        .collect();
    let text = format!("{host}{copies}");
    state.doc().text.set(text.clone());
    state.problems().diagnosed_text.set(text);
    let problems = state.problems().diagnostics.get_untracked();
    let placed = problems.iter().filter(|problem| problem.line.is_some());
    let notes: Vec<_> = problems
        .iter()
        .filter(|problem| problem.code.as_deref() == Some(MORE_PROBLEMS_CODE))
        .collect();
    assert_eq!(placed.count(), MAX_DIAGNOSTICS, "{problems:#?}");
    assert_eq!(notes.len(), 1, "{problems:#?}");
    assert_eq!(notes[0].severity, Severity::Warning);
}
