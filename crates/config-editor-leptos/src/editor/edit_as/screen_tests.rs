//! What the host is told of the screen while editing as another language:
//! whether the text on screen is written back to its document (`in_sync`)
//! through edits, undo and switches; a text kept from an earlier session
//! opened in place of the host's; the language on screen for documents
//! that read the same in every language; and asking for a language again.

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;

/// Replaces the text on screen with `text`, as typing would, and lets the
/// typing pause's conversion run.
fn type_text(state: EditorState, text: &str) {
    let before = state.doc().text.get_untracked();
    let end = before.encode_utf16().count();
    state.commit(Edit::new(before, (0, end), text.to_string(), text.len()));
    pause(state);
}

/// The typing pause: the conversion of the text on screen.
fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

fn in_sync(state: EditorState) -> bool {
    state.sync().in_sync.get_untracked()
}

/// A YAML document edited as TOML, with what `on_change` heard.
fn yaml_as_toml(text: &str) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::YAML, text);
    editor.edit_as = Some(Language::TOML);
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

#[test]
fn in_sync_says_whether_the_text_on_screen_is_written_back() {
    let (_owner, state, heard) = yaml_as_toml("# kept\na: 1\n");
    assert_eq!(state.sync().edit_language.get_untracked(), Language::TOML);
    assert!(in_sync(state), "in sync on load");

    type_text(state, "a = ");
    assert!(!in_sync(state), "invalid TOML is not written back");
    assert_eq!(state.host_document(), None);
    type_text(state, "a = [");
    assert!(!in_sync(state), "still invalid");

    type_text(state, "a = 2\n");
    assert!(in_sync(state), "valid again");
    assert_eq!(
        heard.get_untracked().last().map(String::as_str),
        Some("# kept\na: 2\n")
    );

    // Undo back to an invalid text, then to the one written back.
    let undo = state.history_edit(false).expect("a step to undo");
    state.commit(undo);
    pause(state);
    assert!(!in_sync(state), "undone to invalid text");
    let redo = state.history_edit(true).expect("a step to redo");
    state.commit(redo);
    pause(state);
    assert!(in_sync(state), "redone to the text written back");

    // A new host value is on screen and written back at once.
    type_text(state, "a = ");
    assert!(!in_sync(state));
    super::super::sync::take_host_value(state, "a: 3\n".to_string());
    assert!(in_sync(state), "a new host value is in sync");
}

#[test]
fn in_sync_stays_true_in_the_hosts_language_and_after_a_switch_back() {
    let (_owner, state) = TestEditor::new(Language::YAML, "a: 1\n").mount();
    type_text(state, "a: [");
    assert!(
        in_sync(state),
        "the host's own language is always written back"
    );

    let (_owner, state, _) = yaml_as_toml("a: 1\n");
    type_text(state, "a = ");
    assert!(!in_sync(state));
    // Undo steps back to the YAML text before the switch: the host's own.
    let undo = state.history_edit(false).expect("the edit");
    state.commit(undo);
    pause(state);
    assert!(in_sync(state));
}

#[test]
fn a_kept_text_that_does_not_convert_opens_out_of_sync_and_saves_once_fixed() {
    let host = "# The listener\nlisten: 7835\nname: a\n";
    let mut editor = TestEditor::new(Language::YAML, host);
    editor.edit_as = Some(Language::TOML);
    editor.opening = Some("listen = 7836\nname = \n".to_string());
    let heard = RwSignal::new(Vec::<String>::new());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (_owner, state) = editor.mount();
    assert_eq!(state.sync().edit_language.get_untracked(), Language::TOML);
    assert_eq!(state.doc().text.get_untracked(), "listen = 7836\nname = \n");
    assert!(!in_sync(state), "opens out of sync");
    pause(state);
    assert!(!in_sync(state));
    assert_eq!(state.host_document(), None);
    assert!(heard.get_untracked().is_empty(), "nothing sent yet");
    assert!(
        state.editing().dirty.get_untracked(),
        "the kept text is unsaved"
    );

    type_text(state, "listen = 7836\nname = \"a\"\n");
    assert!(in_sync(state));
    assert_eq!(
        heard.get_untracked(),
        vec!["# The listener\nlisten: 7836\nname: a\n".to_string()],
        "the host's language, its comment kept"
    );
    // Undo starts at the kept text.
    let undo = state.history_edit(false).expect("the fix");
    state.commit(undo);
    assert_eq!(state.doc().text.get_untracked(), "listen = 7836\nname = \n");
    assert!(
        state.history_edit(false).is_none(),
        "nothing before the kept text"
    );
}

#[test]
fn a_kept_text_that_converts_opens_in_sync_and_reaches_the_host() {
    let host = "# The listener\nlisten: 7835\n";
    let mut editor = TestEditor::new(Language::YAML, host);
    editor.edit_as = Some(Language::TOML);
    editor.opening = Some("listen = 7836\n".to_string());
    let heard = RwSignal::new(Vec::<String>::new());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (_owner, state) = editor.mount();
    assert!(in_sync(state));
    // The first conversion (on mount) sends it.
    pause(state);
    assert_eq!(
        heard.get_untracked(),
        vec!["# The listener\nlisten: 7836\n".to_string()]
    );
    assert_eq!(
        state.host_document().as_deref(),
        Some("# The listener\nlisten: 7836\n")
    );

    // The host's own text converted, kept: nothing to send.
    let mut editor = TestEditor::new(Language::YAML, "listen: 7835\n");
    editor.edit_as = Some(Language::TOML);
    editor.opening = Some("listen = 7835\n".to_string());
    let (_owner, state) = editor.mount();
    assert!(in_sync(state));
    assert!(!state.editing().dirty.get_untracked());
}

#[test]
fn a_kept_text_opens_when_the_hosts_text_cannot_be_shown_in_its_language() {
    // TOML has no null: the host's text alone would wait in YAML.
    let mut editor = TestEditor::new(Language::YAML, "a: null\n");
    editor.edit_as = Some(Language::TOML);
    editor.opening = Some("a = 1\n".to_string());
    let (_owner, state) = editor.mount();
    assert_eq!(state.sync().edit_language.get_untracked(), Language::TOML);
    assert_eq!(state.doc().text.get_untracked(), "a = 1\n");
    assert!(in_sync(state));
}

#[test]
fn empty_and_comment_only_documents_show_as_toml() {
    for text in ["", "# only a comment\n"] {
        let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
        let heard = RwSignal::new(Vec::<Language>::new());
        state
            .sync()
            .on_edit_as_change
            .set_value(Some(Callback::new(move |language| {
                heard.update(|heard| heard.push(language));
            })));
        switch_to(state, Language::TOML);
        assert_eq!(
            state.sync().edit_language.get_untracked(),
            Language::TOML,
            "{text:?} on screen as TOML"
        );
        assert_eq!(heard.get_untracked(), vec![Language::TOML], "{text:?}");
        pause(state);
        assert!(in_sync(state), "{text:?}");
    }
}

#[test]
fn asking_again_after_an_undo_switches_again() {
    let (_owner, state) = TestEditor::new(Language::YAML, "a: 1\n").mount();
    let heard = RwSignal::new(Vec::<Language>::new());
    state
        .sync()
        .on_edit_as_change
        .set_value(Some(Callback::new(move |language| {
            heard.update(|heard| heard.push(language));
        })));
    state.ask_edit_as(Language::TOML);
    let undo = state.history_edit(false).expect("the switch");
    state.commit(undo);
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
    state.ask_edit_as(Language::TOML);
    assert_eq!(state.sync().edit_language.get_untracked(), Language::TOML);
    // The language on screen asked for: nothing happens.
    state.ask_edit_as(Language::TOML);
    assert_eq!(
        heard.get_untracked(),
        vec![Language::TOML, Language::YAML, Language::TOML]
    );
}

#[test]
fn asking_for_the_hosts_language_while_the_text_cannot_convert_waits_then_switches() {
    let (_owner, state, heard) = yaml_as_toml("a: 1\n");
    let languages = RwSignal::new(Vec::<Language>::new());
    state
        .sync()
        .on_edit_as_change
        .set_value(Some(Callback::new(move |language| {
            languages.update(|heard| heard.push(language));
        })));
    type_text(state, "a = ");
    state.ask_edit_as(Language::YAML);
    assert_eq!(
        state.sync().edit_language.get_untracked(),
        Language::TOML,
        "waits"
    );
    assert_eq!(
        state.sync().pending_switch.get_untracked(),
        Some(Language::YAML)
    );
    assert!(
        languages.get_untracked().is_empty(),
        "nothing changed on screen"
    );
    // Fixed: the switch waiting is made (as the editor retries it after
    // the edit).
    type_text(state, "a = 5\n");
    assert!(in_sync(state));
    switch_to(state, Language::YAML);
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
    assert_eq!(state.doc().text.get_untracked(), "a: 5\n");
    assert_eq!(languages.get_untracked(), vec![Language::YAML]);
    assert_eq!(
        heard.get_untracked().last().map(String::as_str),
        Some("a: 5\n")
    );
    // Asking for the language on screen ends a switch still waiting.
    type_text(state, "a: [");
    state.ask_edit_as(Language::TOML);
    assert_eq!(
        state.sync().pending_switch.get_untracked(),
        Some(Language::TOML)
    );
    state.ask_edit_as(Language::YAML);
    assert_eq!(state.sync().pending_switch.get_untracked(), None);
}
