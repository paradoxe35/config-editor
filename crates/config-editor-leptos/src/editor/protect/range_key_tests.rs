//! P1 with a host range over part of a key: the range locks the whole
//! field, its name too, as a read-only path does. Typing in the key, at
//! either end of it or in the part the range leaves out, is refused at
//! once with the read-only reason; nothing is held under a half-typed name.
//! A range over the value alone leaves the key free.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::replace;
use crate::strings::default_strings;

/// Each language with keys: its text holding `public_address`.
const CASES: [(Language, &str); 5] = [
    (
        Language::YAML,
        "listen: 7835\npublic_address: tunnel.example.com\n",
    ),
    (
        Language::JSON,
        "{\n  \"listen\": 7835,\n  \"public_address\": \"tunnel.example.com\"\n}\n",
    ),
    (
        Language::TOML,
        "listen = 7835\npublic_address = \"tunnel.example.com\"\n",
    ),
    (
        Language::HCL,
        "listen = 7835\npublic_address = \"tunnel.example.com\"\n",
    ),
    (
        Language::XML,
        "<config>\n  <listen>7835</listen>\n  <public_address>tunnel.example.com</public_address>\n</config>\n",
    ),
];

/// The one-based position of byte `at` of `text`.
fn position(text: &str, at: usize) -> TextPosition {
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    TextPosition::new(line, column)
}

/// An editor of `text` with a host range over the first `locked` in it.
fn mount(language: Language, text: &str, locked: &str) -> (Owner, EditorState) {
    let start = text.find(locked).expect("the locked text");
    let mut editor = TestEditor::new(language, text);
    editor.read_only_ranges = vec![TextRange::new(
        position(text, start),
        position(text, start + locked.len()),
    )];
    editor.mount()
}

fn holds(state: EditorState) -> Vec<String> {
    state
        .protect()
        .protected
        .with_untracked(|protected| protected.held.iter().cloned().collect())
}

/// Types `typed` at byte `at`; whether it was taken, and the refusal's
/// message shown when it was not.
fn typed(state: EditorState, at: usize, typed: &str) -> (bool, Option<String>) {
    let before = state.doc().text.get_untracked();
    replace(state, at..at, typed);
    let taken = state.doc().text.get_untracked() != before;
    (taken, state.protect().tip().map(|tip| tip.message))
}

#[test]
fn typing_in_a_key_a_host_range_touches_is_refused_at_once() {
    let reason = default_strings().read_only_text.clone();
    for (language, text) in CASES {
        let key = text.find("public_address").expect("the key");
        // At its end, at its start, inside it past the range.
        for (at, what) in [(key + 14, "_v4"), (key, "my_"), (key + 9, "x")] {
            let (_owner, state) = mount(language, text, "lic");
            let (taken, tip) = typed(state, at, what);
            assert!(!taken, "{language:?}: {what:?} at {at} is refused");
            assert_eq!(state.doc().text.get_untracked(), text, "{language:?}");
            assert_eq!(
                tip.as_deref(),
                Some(reason.as_str()),
                "{language:?}: {what:?}"
            );
            assert_eq!(
                holds(state),
                Vec::<String>::new(),
                "{language:?}: nothing held"
            );
            assert_eq!(untrack(|| state.save_waits()), None, "{language:?}");
        }
    }
}

#[test]
fn a_range_over_the_value_alone_leaves_its_key_free() {
    for (language, text) in CASES {
        // XML writes the name twice: renaming one breaks the text.
        if language == Language::XML {
            continue;
        }
        let (_owner, state) = mount(language, text, "example");
        let key = text.find("public_address").expect("the key");
        let (taken, _) = typed(state, key, "my_");
        assert!(taken, "{language:?}: the key is not locked");
    }
}
