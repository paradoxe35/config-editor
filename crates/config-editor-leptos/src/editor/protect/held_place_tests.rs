//! A field held that the text no longer has (the host names its path after
//! the user cut its line) has no lock of its own to mark: its "Not saved"
//! problem sits where the lock nearest to it by path starts, else on the
//! first line, so walking to it (F8) shows it there.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::FieldPath;
use super::super::state::{EditorState, Init};
use super::ProtectInit;
use crate::EditorSettings;

fn mount(text: &str, paths: RwSignal<Vec<FieldPath>>) -> EditorState {
    EditorState::new(Init {
        host_language: Language::YAML,
        edit_language: Language::YAML,
        schema: None,
        schema_error: None,
        host_text: text.to_string(),
        baseline: text.to_string(),
        opening: None,
        read_only: Signal::stored(false),
        read_only_reason: Signal::stored(None),
        settings: Signal::stored(EditorSettings::default()),
        host_on_change: None,
        channel: None,
        protect: ProtectInit {
            paths: Some(paths.into()),
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
        key_names: Default::default(),
        history: None,
    })
    .0
}

/// Cuts the line of `text` on screen holding `line`.
fn cut_line(state: EditorState, line: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(line).expect("the line");
    let end = at + line.len() + 1;
    let after = format!("{}{}", &before[..at], &before[end..]);
    let units = |byte: usize| before[..byte].encode_utf16().count();
    assert!(state.commit(Edit::input_units(
        before.clone(),
        (units(at), units(end)),
        after,
        (units(at), units(at)),
        "deleteByCut".into(),
        0.0,
    )));
}

/// The held problems of the text on screen: (line, column, message).
fn held(state: EditorState) -> Vec<(Option<usize>, Option<usize>, String)> {
    let text = state.doc().text.get_untracked();
    untrack(|| {
        state.strings_untracked(|strings| {
            state
                .protect()
                .held_problems(state.behavior(), &text, strings)
                .into_iter()
                .map(|problem| (problem.line, problem.column, problem.message))
                .collect()
        })
    })
}

#[test]
fn a_held_field_the_text_no_longer_has_is_marked_at_the_nearest_lock() {
    let owner = Owner::new();
    owner.set();
    let paths = RwSignal::new(vec![
        FieldPath::from("/name"),
        FieldPath::from("/server/host"),
    ]);
    let state = mount(
        "name: app\nserver:\n  host: a.example.com\n  port: 80\n",
        paths,
    );
    cut_line(state, "  port: 80");
    paths.update(|paths| paths.push(FieldPath::from("/server/port")));
    assert!(state.changes_held(), "the save waits for the port");
    let problems = held(state);
    assert_eq!(problems.len(), 1, "{problems:?}");
    let (line, column, message) = &problems[0];
    assert!(message.contains("/server/port"), "{message}");
    // Where `host`'s lock starts, the field sharing `/server` with it.
    assert_eq!((*line, *column), (Some(3), Some(3)), "{problems:?}");
}

#[test]
fn a_held_field_with_no_lock_left_is_marked_on_the_first_line() {
    let owner = Owner::new();
    owner.set();
    let paths = RwSignal::new(Vec::new());
    let state = mount("name: app\nport: 80\n", paths);
    cut_line(state, "port: 80");
    paths.set(vec![FieldPath::from("/port")]);
    assert!(state.changes_held(), "the save waits for the port");
    let problems = held(state);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(
        (problems[0].0, problems[0].1),
        (Some(1), Some(1)),
        "{problems:?}"
    );
}
