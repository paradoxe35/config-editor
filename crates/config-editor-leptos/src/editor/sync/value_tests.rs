//! A controlled `value` as the editor follows it (`follow_value`): its
//! revision passed back, and a leading U+FEFF read by the byte order mark
//! the host's first text had.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::follow_value;

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
    let text = value.get_untracked();
    state.sync().link(|link| link.echoing(&text, None, None));
    follow_value(
        state,
        value.into(),
        revision.into(),
        super::Lists::default(),
    );
    (owner, state, heard)
}

fn sent_revision(state: EditorState) -> u64 {
    state
        .sync()
        .link(|link| link.echoes.latest_revision())
        .flatten()
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

/// P2: an echo the host's store wrote with LF breaks, of a CRLF document,
/// is skipped and leaves the document's own breaks: the host hears CRLF.
#[test]
fn an_echo_written_with_other_line_breaks_leaves_the_documents_own() {
    let value = RwSignal::new("a: 1\r\nb: 2\r\n".to_string());
    let (_owner, state, heard) = mounted(value, RwSignal::new(None));
    type_at_line_end(state, "x");
    assert_eq!(
        heard.lock().expect("heard").last().map(String::as_str),
        Some("a: 1x\r\nb: 2\r\n")
    );
    value.set("a: 1x\nb: 2\n".to_string());
    assert_eq!(state.doc().text.get_untracked(), "a: 1x\nb: 2\n");
    type_at_line_end(state, "y");
    assert_eq!(
        heard.lock().expect("heard").last().map(String::as_str),
        Some("a: 1xy\r\nb: 2\r\n")
    );
}

/// P2: a host without `echoes` gives the document again written its own
/// way (CRLF, CRLF after the byte order mark it loaded with, mixed
/// breaks): nothing changes and the host hears nothing, but from then on
/// it hears its own form back, byte for byte.
#[test]
fn the_hosts_own_text_written_another_way_is_its_form_from_then_on() {
    for (first, given, heard_back) in [
        ("a: 1\nb: 2\n", "a: 1\r\nb: 2\r\n", "a: 1x\r\nb: 2\r\n"),
        (
            "\u{feff}a: 1\nb: 2\n",
            "\u{feff}a: 1\r\nb: 2\r\n",
            "\u{feff}a: 1x\r\nb: 2\r\n",
        ),
        ("a: 1\nb: 2\n", "a: 1\r\nb: 2\n", "a: 1x\r\nb: 2\n"),
        ("a: 1\r\nb: 2\r\n", "a: 1\nb: 2\n", "a: 1x\nb: 2\n"),
    ] {
        let value = RwSignal::new(first.to_string());
        let heard = Arc::new(Mutex::new(Vec::new()));
        let told = heard.clone();
        let (_owner, state) = TestEditor {
            on_change: Some(Callback::new(move |text: String| {
                told.lock().expect("heard").push(text);
            })),
            ..TestEditor::new(Language::YAML, first)
        }
        .mount();
        follow_value(
            state,
            value.into(),
            RwSignal::new(None).into(),
            super::Lists::default(),
        );
        // Given twice: the second time no news either.
        value.set(given.to_string());
        value.set(given.to_string());
        let shown = state.doc().text.get_untracked();
        assert_eq!(shown, "a: 1\nb: 2\n", "{given:?}");
        assert!(heard.lock().expect("heard").is_empty(), "{given:?}");
        type_at_line_end(state, "x");
        assert_eq!(
            heard.lock().expect("heard").last().map(String::as_str),
            Some(heard_back),
            "{given:?}"
        );
    }
}

/// The locks of the text on screen, as the text they cover.
#[cfg(feature = "protect")]
fn locked(state: EditorState) -> Vec<String> {
    let text = state.doc().text.get_untracked();
    state.protect().ranges.with_untracked(|ranges| {
        ranges
            .iter()
            .map(|range| text[usize::from(range.start())..usize::from(range.end())].to_string())
            .collect()
    })
}

/// Ranges worked out once on the file as loaded, given with the text the
/// editor mounts with (a line added above since): they lock their text,
/// not what their numbers name in the mount's text.
#[cfg(feature = "protect")]
#[test]
fn ranges_named_by_the_file_as_loaded_lock_their_text() {
    use super::super::paths::{TextPosition, TextRange};
    let loaded = "a: 1\nb: 2\n";
    let (_owner, state) = TestEditor {
        read_only_ranges: vec![TextRange::new(
            TextPosition::new(2, 1),
            TextPosition::new(2, 5),
        )],
        read_only_ranges_for: Some(super::RangesFor::Text(loaded.into())),
        ..TestEditor::new(Language::YAML, "top: 0\na: 1\nb: 2\n")
    }
    .mount();
    assert_eq!(locked(state), ["b: 2"]);
}

/// A revision the editor never sent names no text: the list is refused and
/// the editor locks nothing new.
#[cfg(feature = "protect")]
#[test]
fn ranges_named_by_a_revision_never_sent_lock_nothing() {
    use super::super::paths::{TextPosition, TextRange};
    let (_owner, state) = TestEditor {
        read_only_ranges: vec![TextRange::new(
            TextPosition::new(1, 1),
            TextPosition::new(1, 5),
        )],
        read_only_ranges_for: Some(super::RangesFor::Revision(u64::MAX)),
        ..TestEditor::new(Language::YAML, "a: 1\n")
    }
    .mount();
    assert!(locked(state).is_empty());
}
