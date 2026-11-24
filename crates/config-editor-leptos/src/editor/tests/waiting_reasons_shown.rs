//! With the `show_diagnostics` setting off the text's own problems are
//! hidden, but why an action the user asked for waits still shows (marked,
//! counted, reported to `on_problems`): an Edit-as switch the document
//! cannot make yet, an edit the host's language cannot hold, a save held
//! by a read-only field.

use leptos::prelude::*;

use crate::EditorSettings;
use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::paths::FieldPath;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

fn diagnostics_off(mut editor: TestEditor) -> (Owner, EditorState) {
    editor.settings = Signal::stored(EditorSettings {
        show_diagnostics: false,
        ..EditorSettings::default()
    });
    editor.mount()
}

/// The problems shown and the ones `on_problems` hears, after the typing
/// pause checked the text on screen.
fn found(state: EditorState) -> (Vec<String>, Vec<String>) {
    state.check_text_now();
    let messages = |problems: Vec<crate::editor::diagnostics::Diagnostic>| {
        problems
            .into_iter()
            .map(|problem| problem.message)
            .collect::<Vec<_>>()
    };
    (
        messages(state.problems().diagnostics.get_untracked()),
        messages(state.problems().own.get_untracked()),
    )
}

/// Replaces the first `old` of the text on screen with `new`, as typing.
fn change(state: EditorState, old: &str, new: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(old).expect("the text to change");
    let units = |end: usize| before[..end].encode_utf16().count();
    let (start, end) = (units(at), units(at + old.len()));
    let after = format!("{}{new}{}", &before[..at], &before[at + old.len()..]);
    let caret = start + new.encode_utf16().count();
    state.commit(Edit::new_units(before, (start, end), after, caret));
}

#[test]
fn a_waiting_edit_as_switch_says_why_with_diagnostics_off() {
    let text = "{\n  \"a\": null\n}\n";
    let (_owner, state) = diagnostics_off(TestEditor::new(Language::JSON, text));
    state.ask_edit_as(Language::TOML);
    assert_eq!(
        state.sync().pending_switch.get_untracked(),
        Some(Language::TOML),
        "TOML has no null: the switch waits"
    );
    let (shown, heard) = found(state);
    assert!(
        matches!(shown.as_slice(), [reason] if reason.starts_with("Cannot switch to TOML yet")),
        "shown: {shown:?}"
    );
    assert_eq!(heard, shown, "on_problems hears the reason too");
    let placed = state.problems().diagnostics.with_untracked(|problems| {
        problems
            .iter()
            .all(|problem| problem.line.is_some() && problem.column.is_some())
    });
    assert!(placed, "on the value in the way");
}

#[test]
fn the_texts_own_problems_stay_hidden_with_diagnostics_off() {
    let (_owner, state) = diagnostics_off(TestEditor::new(Language::JSON, "{\n  \"a\": \n"));
    assert_eq!(found(state), (Vec::new(), Vec::new()));
}

#[test]
fn a_held_save_says_why_with_diagnostics_off() {
    let mut editor = TestEditor::new(Language::YAML, "id: srv-42\nlisten: 1\n");
    editor.read_only_paths = vec![FieldPath::from("/id")];
    let (_owner, state) = diagnostics_off(editor);
    change(state, "id: srv-42", "# id: srv-42");
    let (shown, heard) = found(state);
    assert!(
        matches!(shown.as_slice(), [reason] if reason.starts_with("Not saved: the read-only field /id")),
        "shown: {shown:?}"
    );
    assert_eq!(heard, shown);
}

/// An edit in the view the host's language cannot hold (XML has no
/// element named `x/y`) waits with its reason.
#[cfg(feature = "xml")]
#[test]
fn an_edit_the_host_cannot_hold_says_why_with_diagnostics_off() {
    let mut editor = TestEditor::new(Language::XML, "<a>1</a>\n");
    editor.edit_as = Some(Language::YAML);
    let (_owner, state) = diagnostics_off(editor);
    assert_eq!(state.doc().text.get_untracked(), "a: 1\n");
    change(state, "a: 1\n", "a: 1\nx/y: 2\n");
    let (shown, _) = found(state);
    assert!(
        matches!(shown.as_slice(), [reason] if reason.contains("not saved") || reason.contains("Not saved")),
        "shown: {shown:?}"
    );
}
