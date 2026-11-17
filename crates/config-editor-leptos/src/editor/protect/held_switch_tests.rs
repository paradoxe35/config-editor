//! P1: a change held in the host's language (a locked field reads
//! otherwise) is not the host's text: a switch to an Edit-as view waits
//! (a view of it would save over a host text it does not show), the
//! hold's reason stays shown, and the host never hears a text where the
//! locked field reads otherwise; once an edit ends the hold, the switch
//! is made.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;

const HOST: &str =
    "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n";

/// The host's text with `listen`'s value locked by a range, and what
/// `on_change` heard.
fn mount() -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::new());
    let mut editor = TestEditor::new(Language::YAML, HOST);
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(2, 9),
        TextPosition::new(2, 13),
    )];
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|all| all.push(text))
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// Replaces UTF-16 `start..end` (all ASCII here) of the text on screen.
fn replace(state: EditorState, (start, end): (usize, usize), with: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..start], &before[end..]);
    let caret = start + with.len();
    state.commit(Edit::input_units(
        before,
        (start, end),
        after,
        (caret, caret),
        "insertText".into(),
        0.0,
    ))
}

fn listen_reads_as_given(text: &str) -> bool {
    Language::YAML
        .parse(text)
        .is_ok_and(|value| value.pointer("/listen") == Some(&serde_json::json!(7835)))
}

/// Typing over the comment's end and `listen`'s key comments the key out:
/// the change is held, the host hears nothing.
fn hold(state: EditorState, heard: RwSignal<Vec<String>>) {
    assert!(
        replace(state, (23, 35), "a"),
        "the edit leaves the locked text alone"
    );
    assert_eq!(
        state.doc().text.get_untracked(),
        "# Tunnel server configuan: 7835\npublic_address: tunnel.example.com\n"
    );
    assert!(state.changes_held());
    assert!(heard.get_untracked().is_empty());
}

#[test]
fn a_switch_from_a_held_change_waits_until_the_hold_ends() {
    for view in [Language::HCL, Language::JSON, Language::TOML] {
        let (_owner, state, heard) = mount();
        hold(state, heard);
        switch_to(state, view);
        assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
        assert_eq!(state.sync().pending_switch.get_untracked(), Some(view));
        assert_eq!(state.sync().host_text.get_value(), HOST, "{view:?}");
        assert!(state.changes_held(), "{view:?}: still held");
        // Asking for the host's language again ends the wait.
        state.ask_edit_as(Language::YAML);
        assert_eq!(state.sync().pending_switch.get_untracked(), None);
        assert!(heard.get_untracked().is_empty(), "{view:?}");
        // The key typed back ends the hold: the switch is made.
        switch_to(state, view);
        assert!(replace(state, (23, 24), "ration\nliste"));
        assert!(!state.changes_held());
        switch_to(state, view);
        assert_eq!(state.sync().edit_language.get_untracked(), view);
        assert!(
            heard
                .get_untracked()
                .iter()
                .all(|text| listen_reads_as_given(text)),
            "{view:?}: heard {:?}",
            heard.get_untracked()
        );
    }
}
