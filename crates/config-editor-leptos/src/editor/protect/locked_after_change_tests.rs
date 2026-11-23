//! P1 with a path the host names after the user changed its field: the
//! field's text is the user's, not the host's, so it locks nothing on
//! screen and the save waits with a reason; every way back to the host's
//! text (deleting what was typed, undo, typing the host's text over it,
//! the line cut and typed again) is taken, and once the field reads as the
//! host gave it the hold is released and the field is locked again.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::paths::FieldPath;
use super::super::state::{EditorState, Init};
use super::ProtectInit;
use super::guarantee_tests::replace;
use crate::EditorSettings;

/// Each language's text with a field `/…name` of value `edge`, and its path.
const CASES: [(Language, &str, &str); 5] = [
    (Language::YAML, "name: edge\nport: 80\n", "/name"),
    (
        Language::JSON,
        "{\n  \"name\": \"edge\",\n  \"port\": 80\n}\n",
        "/name",
    ),
    (Language::TOML, "name = \"edge\"\nport = 80\n", "/name"),
    (Language::HCL, "name = \"edge\"\nport = 80\n", "/name"),
    (
        Language::XML,
        "<config>\n  <name>edge</name>\n  <port>80</port>\n</config>\n",
        "/config/name",
    ),
];

/// An editor of `text` whose read-only paths the test sets.
fn mount(language: Language, text: &str) -> (Owner, EditorState, RwSignal<Vec<FieldPath>>) {
    let owner = Owner::new();
    owner.set();
    let paths = RwSignal::new(Vec::<FieldPath>::new());
    let (state, _) = EditorState::new(Init {
        host_language: language,
        edit_language: language,
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
    });
    (owner, state, paths)
}

/// Replaces the first `old` of the text on screen with `new`; whether the
/// edit was taken.
fn change(state: EditorState, old: &str, new: &str) -> bool {
    let text = state.doc().text.get_untracked();
    let at = text.find(old).expect("the text to change");
    replace(state, at..at + old.len(), new);
    state.doc().text.get_untracked() != text
}

/// An editor whose field `edge` was changed to `edgeX`, then locked by the
/// host: held, the save waiting.
fn locked_after_change(
    language: Language,
    text: &str,
    pointer: &str,
) -> (Owner, EditorState, RwSignal<Vec<FieldPath>>) {
    let (owner, state, paths) = mount(language, text);
    assert!(
        change(state, "edge", "edgeX"),
        "{language:?}: typing is taken"
    );
    paths.set(vec![FieldPath::from_pointer(pointer)]);
    assert!(
        untrack(|| state.save_waits()).is_some_and(|why| why.contains("before the host made it")),
        "{language:?}: the save waits for the change made before the lock"
    );
    (owner, state, paths)
}

/// Asserts the field reads as the host gave it, nothing held, and locked
/// again: an edit into it is refused.
fn assert_back_and_locked(language: Language, state: EditorState, text: &str, how: &str) {
    assert_eq!(
        state.doc().text.get_untracked(),
        text,
        "{language:?} {how}: the host's text"
    );
    assert_eq!(
        untrack(|| state.save_waits()),
        None,
        "{language:?} {how}: the save no longer waits"
    );
    assert!(
        !untrack(|| state.changes_held()),
        "{language:?} {how}: nothing is held"
    );
    assert!(
        !change(state, "edge", "edgeY"),
        "{language:?} {how}: the field is locked again"
    );
}

#[test]
fn deleting_what_was_typed_before_the_lock_is_taken_and_releases_the_hold() {
    for (language, text, pointer) in CASES {
        let (_owner, state, _paths) = locked_after_change(language, text, pointer);
        assert!(
            change(state, "edgeX", "edge"),
            "{language:?}: deleting the X is taken"
        );
        assert_back_and_locked(language, state, text, "after deleting");
    }
}

#[test]
fn undoing_what_was_typed_before_the_lock_is_taken_and_releases_the_hold() {
    for (language, text, pointer) in CASES {
        let (_owner, state, _paths) = locked_after_change(language, text, pointer);
        let undo = state.history_edit(false).expect("an edit to undo");
        assert!(state.commit(undo), "{language:?}: the undo is taken");
        assert_back_and_locked(language, state, text, "after undo");
    }
}

#[test]
fn typing_the_hosts_text_over_the_changed_value_is_taken_step_by_step() {
    for (language, text, pointer) in CASES {
        let (_owner, state, _paths) = locked_after_change(language, text, pointer);
        // Selected and typed over, one character at a time: the steps
        // between read otherwise and stay held, the last reads as the host's.
        let at = text.find("edge").expect("the value");
        assert!(change(state, "edgeX", "e"), "{language:?}: the first key");
        assert!(
            untrack(|| state.save_waits()).is_some(),
            "{language:?}: still held while it reads otherwise"
        );
        for (end, typed) in [(1, "d"), (2, "g"), (3, "e")] {
            let before = state.doc().text.get_untracked();
            replace(state, at + end..at + end, typed);
            assert_ne!(
                state.doc().text.get_untracked(),
                before,
                "{language:?}: typing {typed:?} is taken"
            );
        }
        assert_back_and_locked(language, state, text, "after typing it again");
    }
}

#[test]
fn a_line_cut_and_typed_again_as_the_host_gave_it_releases_the_hold() {
    let text = "name: edge\nport: 80\n";
    let (_owner, state, _paths) = locked_after_change(Language::YAML, text, "/name");
    assert!(change(state, "name: edgeX\n", ""), "the line cut is taken");
    assert_eq!(state.doc().text.get_untracked(), "port: 80\n");
    assert!(untrack(|| state.save_waits()).is_some(), "still held");
    let typed = "name: edge\n";
    for end in 1..=typed.len() {
        let before = state.doc().text.get_untracked();
        replace(state, end - 1..end - 1, &typed[end - 1..end]);
        assert_ne!(
            state.doc().text.get_untracked(),
            before,
            "typing {:?} is taken",
            &typed[..end]
        );
    }
    assert_back_and_locked(Language::YAML, state, text, "after typing the line");
}
