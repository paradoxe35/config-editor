//! A controlled `value` as the editor follows it (`follow_value`): its
//! revision passed back, and a leading U+FEFF read by the byte order mark
//! the host's first text had.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::{Echoes, follow_value};

/// Types `text` at the end of the document's first line.
fn type_at_line_end(state: EditorState, text: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find('\n').unwrap_or(before.len());
    let mut after = before.clone();
    after.insert_str(at, text);
    let end = at + text.len();
    let typed = Edit::input_units(
        before,
        (at, at),
        after,
        (end, end),
        "insertText".into(),
        0.0,
    );
    assert!(state.commit(typed));
}

/// An editor with `echoes`, following `value` and `revision` as the
/// component does; and what `on_change` heard.
fn mounted(
    value: RwSignal<String>,
    revision: RwSignal<Option<u64>>,
) -> (Owner, EditorState, Arc<Mutex<Vec<String>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let told = heard.clone();
    let (owner, state) = TestEditor {
        on_change: Some(Callback::new(move |text: String| {
            told.lock().expect("heard").push(text);
        })),
        ..TestEditor::new(Language::YAML, &value.get_untracked())
    }
    .mount();
    state.sync().echoes.update_value(|echoes| {
        *echoes = Echoes::new(true);
        echoes.given(&value.get_untracked());
    });
    follow_value(state, value.into(), revision.into());
    (owner, state, heard)
}

fn sent_revision(state: EditorState) -> u64 {
    state
        .sync()
        .echoes
        .with_value(Echoes::latest_revision)
        .expect("a text on its way")
}

/// P4: a Leptos host passing each `on_change` text back with its revision
/// (the `value_revision` prop) has its echoes matched exactly: a late echo
/// of a confirmed revision is no news, and a host behind but catching up
/// is not sent the document again.
#[test]
fn a_late_echo_with_its_revision_is_no_news() {
    let value = RwSignal::new("a: 1\n".to_string());
    let revision = RwSignal::new(None);
    let (_owner, state, heard) = mounted(value, revision);
    type_at_line_end(state, "x");
    let x = sent_revision(state);
    type_at_line_end(state, "y");
    let xy = sent_revision(state);
    assert_eq!(heard.lock().expect("heard").len(), 2);
    // Handed back in order, each with its revision: behind, catching up.
    revision.set(Some(x));
    value.set("a: 1x\n".to_string());
    revision.set(Some(xy));
    value.set("a: 1xy\n".to_string());
    // A late copy of the first, again with its revision: confirmed
    // already, so no news. By its text alone it came back already, so it
    // would be the host's own and replace the document.
    revision.set(Some(x));
    value.set("a: 1x\n".to_string());
    assert_eq!(state.doc().text.get_untracked(), "a: 1xy\n");
    assert_eq!(heard.lock().expect("heard").len(), 2);
    // A text of the host's own, with no revision, applies.
    revision.set(None);
    value.set("b: 2\n".to_string());
    assert_eq!(state.doc().text.get_untracked(), "b: 2\n");
}

/// P2: the byte order mark is the host's first text's, recorded once: a
/// later text starting with U+FEFF loses it as that mark only when the
/// first text had one; without one, the U+FEFF is a character of the
/// document, kept on screen and sent back.
#[test]
fn a_later_text_starting_with_u_feff_is_read_by_the_first_texts_mark() {
    // No mark at load: a U+FEFF the host gives later is text.
    let value = RwSignal::new("a: 1\n".to_string());
    let (_owner, state, _) = mounted(value, RwSignal::new(None));
    value.set("\u{feff}a: 2\n".to_string());
    assert_eq!(state.doc().text.get_untracked(), "\u{feff}a: 2\n");
    assert_eq!(
        state
            .sync()
            .line_endings
            .restore("\u{feff}a: 2\n".to_string()),
        "\u{feff}a: 2\n"
    );
    // A mark at load: the same text's U+FEFF is that mark, and the host
    // hears it back once.
    let value = RwSignal::new("\u{feff}a: 1\n".to_string());
    let (_owner, state, _) = mounted(value, RwSignal::new(None));
    value.set("\u{feff}a: 2\n".to_string());
    assert_eq!(state.doc().text.get_untracked(), "a: 2\n");
    assert_eq!(
        state.sync().line_endings.restore("a: 2\n".to_string()),
        "\u{feff}a: 2\n"
    );
}
