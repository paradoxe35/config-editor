//! P4 with a lock: an edit whose locked field reads otherwise (its key
//! typed into) is held, the host hearing nothing; undo all the way back to
//! the opening text releases it, and the host hears that text.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::utf16;

const TEXT: &str = "listen: 7835\npublic_address: tunnel.example.com\n";

fn type_at(state: EditorState, at: usize, typed: &str, kind: &str) {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{typed}{}", &before[..at], &before[at..]);
    let start = utf16(&before, at);
    let caret = start + typed.encode_utf16().count();
    state.commit(Edit::input_units(
        before,
        (start, start),
        after,
        (caret, caret),
        kind.into(),
        0.0,
    ));
}

#[test]
fn undo_all_after_a_held_edit_gives_the_host_the_opening_text() {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::YAML, TEXT);
    // The end of `public_address`'s value.
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(2, 31),
        TextPosition::new(3, 1),
    )];
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (_owner, state) = editor.mount();
    type_at(state, 13, "\n", "insertLineBreak");
    type_at(state, 14, "t", "insertText");
    assert_eq!(
        state.doc().text.get_untracked(),
        "listen: 7835\n\ntpublic_address: tunnel.example.com\n"
    );
    while let Some(edit) = state.history_edit(false) {
        if !state.commit(edit) {
            break;
        }
    }
    assert_eq!(state.doc().text.get_untracked(), TEXT);
    let last = heard.with_untracked(|heard| heard.last().cloned());
    assert_eq!(
        last.as_deref(),
        Some(TEXT),
        "heard {:?}",
        heard.get_untracked()
    );
}
