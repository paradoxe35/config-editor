//! Comments and whitespace around a locked field stay free (README): a
//! comment typed after a locked block's header, a key at a time, is taken
//! as one after a locked scalar or inside the block is; the block's text
//! stays as it was.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::paths::FieldPath;
use super::super::save_matrix::report;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::replace;

/// Each language with a comment syntax: a locked block, the end of its
/// header line, and a comment to type there.
const CASES: [(Language, &str, &str, &str); 4] = [
    (
        Language::YAML,
        "listen: 7835\nwebsocket:\n  path: /ws\n  ping: 30\nname: edge\n",
        "websocket:",
        " # c",
    ),
    (
        Language::TOML,
        "listen = 7835\n\n[websocket]\npath = \"/ws\"\nping = 30\n",
        "[websocket]",
        " # c",
    ),
    (
        Language::HCL,
        "listen = 7835\nwebsocket {\n  path = \"/ws\"\n  ping = 30\n}\n",
        "websocket {",
        " # c",
    ),
    (
        Language::JSON,
        "{\n  \"listen\": 7835,\n  \"websocket\": {\n    \"path\": \"/ws\",\n    \"ping\": 30\n  }\n}\n",
        "\"websocket\": {",
        " ",
    ),
];

fn mount(language: Language, text: &str) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(language, text);
    editor.read_only_paths = vec![FieldPath::from("/listen"), FieldPath::from("/websocket")];
    editor.mount()
}

#[test]
fn a_comment_after_a_locked_blocks_header_is_taken_a_key_at_a_time() {
    let mut failures = Vec::new();
    for (language, text, header, comment) in CASES {
        let (_owner, state) = mount(language, text);
        let mut at = text.find(header).expect("the header") + header.len();
        for character in comment.chars() {
            let before = state.doc().text.get_untracked();
            replace(state, at..at, &character.to_string());
            if state.doc().text.get_untracked() == before {
                failures.push(format!(
                    "{language:?}: {character:?} of {comment:?} refused"
                ));
                break;
            }
            at += character.len_utf8();
        }
        if untrack(|| state.changes_held()) {
            failures.push(format!("{language:?}: held after the comment"));
        }
    }
    report(failures);
}
