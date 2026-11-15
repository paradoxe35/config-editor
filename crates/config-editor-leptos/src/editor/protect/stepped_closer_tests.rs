//! A second copy of a read-only field's key typed in its block, a key at
//! a time, is refused as a whole entry whether or not the editor writes
//! closers: with auto-closing on, the `"` written after `"password` is
//! typed over before the refused `:`, and the entry typed before it is
//! taken back all the same, with the keys typed after it refused too (QA
//! 37: JSON kept `"password" "x",`). In every language, for read-only
//! paths, the schema's `readOnly` and read-only ranges.

use leptos::prelude::*;

use super::super::Language;
use super::super::diagnostics::build_schema;
use super::super::edit::Edit;
use super::super::keyboard::{paired_edit, replace_selection};
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::save_matrix::report;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::utf16;

/// A document with a read-only `password` (on line `line`), the text a new
/// line is opened after (with its indent), and the duplicate entry typed.
struct Case {
    language: Language,
    text: &'static str,
    locked: &'static str,
    line: usize,
    after: &'static str,
    indent: &'static str,
    entry: &'static str,
}

const CASES: &[Case] = &[
    Case {
        language: Language::YAML,
        text: "db:\n  user: app\n  password: hunter2\n",
        locked: "/db/password",
        line: 3,
        after: "user: app",
        indent: "  ",
        entry: "password: x",
    },
    Case {
        language: Language::JSON,
        text: "{\n  \"db\": {\n    \"user\": \"app\",\n    \"password\": \"hunter2\"\n  }\n}\n",
        locked: "/db/password",
        line: 4,
        after: "\"app\",",
        indent: "    ",
        entry: "\"password\": \"x\",",
    },
    Case {
        language: Language::TOML,
        text: "[db]\nuser = \"app\"\npassword = \"hunter2\"\n",
        locked: "/db/password",
        line: 3,
        after: "user = \"app\"",
        indent: "",
        entry: "password = \"x\"",
    },
    Case {
        language: Language::HCL,
        text: "db {\n  user = \"app\"\n  password = \"hunter2\"\n}\n",
        locked: "/db/password",
        line: 3,
        after: "user = \"app\"",
        indent: "  ",
        entry: "password = \"x\"",
    },
    Case {
        language: Language::XML,
        text: "<config>\n  <db>\n    <user>app</user>\n    <password>hunter2</password>\n  </db>\n</config>\n",
        locked: "/config/db/password",
        line: 4,
        after: "<user>app</user>",
        indent: "    ",
        entry: "<password>x</password>",
    },
];

#[derive(Clone, Copy, Debug)]
enum Lock {
    Path,
    Schema,
    Range,
}

/// A schema marking the field at `pointer` read-only.
fn schema(pointer: &str) -> String {
    let mut schema = r#"{"type": "string", "readOnly": true}"#.to_string();
    for key in pointer
        .split('/')
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        schema = format!(r#"{{"type": "object", "properties": {{"{key}": {schema}}}}}"#);
    }
    schema
}

fn mount(case: &Case, lock: Lock) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(case.language, case.text);
    match lock {
        Lock::Path => editor.read_only_paths = vec![FieldPath::from(case.locked)],
        Lock::Schema => {
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(case.locked)).unwrap(),
            ));
        }
        Lock::Range => {
            let length = case
                .text
                .split('\n')
                .nth(case.line - 1)
                .unwrap()
                .chars()
                .count();
            editor.read_only_ranges = vec![TextRange {
                start: TextPosition {
                    line: case.line,
                    column: 1,
                },
                end: TextPosition {
                    line: case.line,
                    column: length + 1,
                },
            }];
        }
    }
    editor.mount()
}

/// Types `typed` at byte `at` a key at a time, as the editor's keys do:
/// with `pairs`, an opening bracket or quote writes its closer ahead of
/// the caret, and typing that closer steps over it (as the keys tell the
/// guard). A refused key leaves the caret where the editor put it.
fn type_at(state: EditorState, at: usize, typed: &str, pairs: bool) {
    let behavior = state.behavior();
    let mut caret = utf16(&state.doc().text.get_untracked(), at);
    let mut closers: Vec<usize> = Vec::new();
    for character in typed.chars() {
        let before = state.doc().text.get_untracked();
        let next = before.encode_utf16().nth(caret);
        if pairs && closers.contains(&caret) && next == Some(character as u16) {
            closers.retain(|at| *at != caret);
            state.note_stepped_over(caret, caret + 1);
            caret += 1;
            continue;
        }
        let paired = if pairs {
            paired_edit(&before, caret, caret, character, behavior)
        } else {
            None
        };
        let (after, new_caret, closed) = match paired {
            Some((after, at, _)) => (after, at, true),
            None => {
                let (after, at) = replace_selection(&before, caret, caret, &character.to_string());
                (after, at, false)
            }
        };
        let kind = if character == '\n' {
            "insertLineBreak"
        } else {
            "insertText"
        };
        let edit = Edit::input(
            before,
            (caret, caret),
            after,
            (new_caret, new_caret),
            kind.into(),
            0.0,
        );
        if !state.commit(edit) {
            caret = state.doc().caret.get_untracked();
            closers.retain(|at| *at < caret);
            continue;
        }
        for at in &mut closers {
            if *at >= caret {
                *at += if closed { 2 } else { 1 };
            }
        }
        if closed {
            closers.push(new_caret);
        }
        caret = new_caret;
    }
}

#[test]
fn a_second_copy_of_a_locked_key_is_refused_whole_with_closers_or_without() {
    let mut failures = Vec::new();
    for case in CASES {
        for lock in [Lock::Path, Lock::Schema, Lock::Range] {
            for pairs in [false, true] {
                let (_owner, state) = mount(case, lock);
                let at = case.text.find(case.after).unwrap() + case.after.len();
                let opened = format!("\n{}", case.indent);
                type_at(state, at, &format!("{opened}{}", case.entry), pairs);
                let now = state.doc().text.get_untracked();
                let wanted = format!("{}{opened}{}", &case.text[..at], &case.text[at..]);
                if now != wanted {
                    failures.push(format!(
                        "{} locked by {lock:?}, pairs {pairs}: left\n{now}",
                        case.language
                    ));
                }
            }
        }
    }
    report(failures);
}
