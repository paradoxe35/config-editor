//! A key renamed in a view while its value changes too, beside other
//! edits, is the same entry: its links tell it (P2/P3). It keeps its line,
//! its comment and the form the host writes it in, where comparing values
//! would read a key removed and another added.

use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::sync::convert_now;
use crate::editor::test_state::TestEditor;
use leptos::prelude::*;

fn mount(host: Language, view: Language, text: &str) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(host, text);
    editor.edit_as = Some(view);
    editor.mount()
}

/// The view's text with its first `old` replaced by `new`, typed over a
/// selection of it.
fn typed(state: EditorState, old: &str, new: &str) {
    let before = state.doc().text.get_untracked();
    let at = before
        .find(old)
        .unwrap_or_else(|| panic!("`{old}` in {before}"));
    let text = before.replacen(old, new, 1);
    let units = |text: &str| text.encode_utf16().count();
    let start = units(&before[..at]);
    let end = start + units(old);
    let caret = start + units(new);
    state.commit(Edit::new(before, (start, end), text, caret));
}

fn pause(state: EditorState) -> String {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
    state.sync().host_text.get_value()
}

#[test]
fn a_key_renamed_while_its_value_changes_keeps_its_line_and_comment() {
    let host = "port: 0x1F90 # the listener\nname: web\n";
    let (_owner, state) = mount(Language::YAML, Language::JSON, host);
    typed(state, "\"port\"", "\"listen\"");
    typed(state, "8080", "8083");
    typed(state, "\"web\"", "\"web\",\n  \"debug\": true");
    let saved = pause(state);
    assert_eq!(
        saved,
        "listen: 0x1F93 # the listener\nname: web\ndebug: true\n"
    );
}
