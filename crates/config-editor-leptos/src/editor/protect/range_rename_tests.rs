//! P1 with a host range: a key holding the field the range covers, renamed
//! one keystroke at a time, holds that field once, under the path the
//! host's text gives it (never one hold per half-typed name), and the
//! warning says so in a sentence; the name typed back releases it.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::replace;
use crate::strings::default_strings;

/// Each language's text with a list `ports` under a block labelled `web`,
/// the line of its locked item, and where typing after the label goes.
const CASES: [(Language, &str, usize, &str); 4] = [
    (
        Language::YAML,
        "services:\n  web:\n    image: nginx\n    ports:\n      - \"80:80\"\n",
        5,
        "  web",
    ),
    (
        Language::JSON,
        "{\n  \"services\": {\n    \"web\": {\n      \"image\": \"nginx\",\n      \"ports\": [\"80:80\"]\n    }\n  }\n}\n",
        5,
        "\"web",
    ),
    (
        Language::TOML,
        "[services.web]\nimage = \"nginx\"\nports = [\"80:80\"]\n",
        3,
        ".web",
    ),
    (
        Language::HCL,
        "service \"web\" {\n  image = \"nginx\"\n  ports = [\"80:80\"]\n}\n",
        3,
        "\"web",
    ),
];

/// The pointers held, in order.
fn holds(state: EditorState) -> Vec<String> {
    state
        .protect()
        .protected
        .with_untracked(|protected| protected.held.iter().cloned().collect())
}

/// The warnings starting "Not saved".
fn warnings(state: EditorState) -> Vec<String> {
    let text = state.doc().text.get_untracked();
    untrack(|| {
        state
            .protect()
            .held_problems(state.behavior(), &text, default_strings())
    })
    .into_iter()
    .map(|problem| problem.message)
    .filter(|message| message.starts_with("Not saved"))
    .collect()
}

/// An editor of `text` whose line `line` is a host range.
fn mount(language: Language, text: &str, line: usize) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(language, text);
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(line, 1),
        TextPosition::new(line + 1, 1),
    )];
    editor.mount()
}

#[test]
fn a_key_renamed_keystroke_by_keystroke_holds_the_hosts_field_once() {
    for (language, text, line, label) in CASES {
        let (_owner, state) = mount(language, text, line);
        let at = text.find(label).expect("the label") + label.len();
        let host = holds_after_typing(state, at, "app");
        assert_eq!(host.len(), 1, "{language:?}: one hold, {host:?}");
        let held = &host[0];
        assert!(
            held.contains("/web") && !held.contains("/weba"),
            "{language:?}: the host's path, {held:?}"
        );
        let warned = warnings(state);
        assert_eq!(warned.len(), 1, "{language:?}: one warning, {warned:#?}");
        assert_eq!(
            warned[0],
            format!(
                "Not saved: a key holding the read-only field {held} was renamed; give the key its old name back to save"
            ),
            "{language:?}"
        );
        assert_eq!(
            untrack(|| state.save_waits()).as_ref(),
            Some(&warned[0]),
            "{language:?}: the save gives the same reason"
        );
        // Typed back one key at a time: released.
        for end in (at..at + 3).rev() {
            replace(state, end..end + 1, "");
        }
        assert_eq!(state.doc().text.get_untracked(), text, "{language:?}");
        assert_eq!(holds(state), Vec::<String>::new(), "{language:?}");
        assert_eq!(untrack(|| state.save_waits()), None, "{language:?}");
    }
}

/// Types `typed` at byte `at` one character at a time; the holds after.
fn holds_after_typing(state: EditorState, at: usize, typed: &str) -> Vec<String> {
    for (index, character) in typed.char_indices() {
        let before = state.doc().text.get_untracked();
        replace(state, at + index..at + index, &character.to_string());
        assert_ne!(state.doc().text.get_untracked(), before, "typing is taken");
    }
    holds(state)
}
