//! P1: a range the host gives over text typed in an Edit-as view (or in
//! its own language) locks that text: an undo of the typing goes back to a
//! text the host had before, which lacks it, so the undo's save waits
//! with its reason; the host keeps the locked text. Typed at the start of a
//! key and of a value, as an ASCII letter and as characters whose bytes,
//! UTF-16 units and characters differ (`日`, `😀`, `é`).

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::EditorSettings;
use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::{EditorState, Init};
use super::super::sync::convert_now;
use super::ProtectInit;

const HOST: &str = "listen: 7835\npublic_address: tunnel.example.com\n";

fn mount(view: Language, ranges: RwSignal<Vec<TextRange>>) -> EditorState {
    let state = EditorState::new(Init {
        host_language: Language::YAML,
        edit_language: view,
        schema: None,
        schema_error: None,
        host_text: HOST.to_string(),
        baseline: HOST.to_string(),
        opening: None,
        read_only: Signal::stored(false),
        read_only_reason: Signal::stored(None),
        settings: Signal::stored(EditorSettings::default()),
        host_on_change: None,
        channel: None,
        protect: ProtectInit {
            paths: None,
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
        key_names: Default::default(),
    })
    .0;
    crate::editor::sync::follow_lists(
        state,
        crate::editor::sync::Lists {
            read_only_ranges: Some(ranges.into()),
            ..crate::editor::sync::Lists::default()
        },
    );
    state
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

fn host(state: EditorState) -> String {
    state.sync().host_text.get_value()
}

fn pause(state: EditorState) {
    let language = state.sync().edit_language.get_untracked();
    convert_now(state, shown(state), language);
}

/// Types `typed` at byte `at` of the text on screen in one input.
fn type_at(state: EditorState, at: usize, typed: &str) -> bool {
    let before = shown(state);
    let after = format!("{}{typed}{}", &before[..at], &before[at..]);
    let start = before[..at].encode_utf16().count();
    let caret = start + typed.encode_utf16().count();
    state.commit(Edit::input_units(
        before,
        (start, start),
        after,
        (caret, caret),
        "insertText".into(),
        0.0,
    ))
}

/// In a YAML host and each view of it, a wide character typed at the start of a
/// line or of a value is saved; the host then locks the whole lines
/// holding it; an undo of the typing leaves the locked text in the host.
#[test]
fn a_range_given_over_typed_wide_text_holds_it_through_undo_in_every_view() {
    let mut failures = Vec::new();
    for view in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        for wide in ["x", "日", "😀", "é"] {
            for spot in ["listen", "tunnel"] {
                let owner = Owner::new();
                owner.set();
                let ranges = RwSignal::new(Vec::new());
                let state = mount(view, ranges);
                let Some(at) = shown(state).find(spot) else {
                    continue;
                };
                if !type_at(state, at, wide) {
                    continue;
                }
                pause(state);
                let saved = host(state);
                if !saved.contains(wide) {
                    // The view's language does not read the text typed.
                    continue;
                }
                // The whole lines from the first to the one with the
                // address, as the host counts them.
                let lines = saved.lines().count();
                ranges.set(vec![TextRange::new(
                    TextPosition::new(1, 1),
                    TextPosition::new(lines + 1, 1),
                )]);
                state.place_ranges();
                state.locks_changed_by_host();
                let locked = saved.clone();
                if let Some(edit) = state.history_edit(false) {
                    state.commit(edit);
                }
                pause(state);
                let after = host(state);
                if !after.contains(&locked) {
                    failures.push(format!(
                        "{view} {wide:?} before {spot}: locked {locked:?}, host {after:?}, view {:?}",
                        shown(state)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
