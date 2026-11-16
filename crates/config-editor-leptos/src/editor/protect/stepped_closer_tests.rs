//! A second copy of a read-only field's key typed in its block, a key at
//! a time, with auto-closing on and off: no span's text changes, so every
//! key is taken (the guard reads no half-typed names: QA 37 left JSON's
//! `"password" "x",` when the `:` was refused after the stepped-over `"`).
//! The locked value keeps its text and still refuses typing, and the save
//! waits while the duplicate is there (it does not parse, or it shadows
//! the field). In every language, for read-only paths, the schema's
//! `readOnly` and read-only ranges.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::host_editing::SaveRequest;
use super::super::keyboard::{paired_text, replace_selection};
use super::super::language::Repetition;
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::save_matrix::report;
use super::super::schema::build_schema;
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
    editor.on_save = Some(Callback::new(|_: SaveRequest| {}));
    editor.mount()
}

/// Types `typed` at byte `at` a key at a time, as the editor's keys do:
/// with `pairs`, an opening bracket or quote writes its closer ahead of
/// the caret, and typing that closer steps over it. A refused key leaves
/// the caret where the editor put it.
pub(super) fn type_at(state: EditorState, at: usize, typed: &str, pairs: bool) {
    let behavior = state.behavior();
    let mut caret = utf16(&state.doc().text.get_untracked(), at);
    let mut closers: Vec<usize> = Vec::new();
    for character in typed.chars() {
        let before = state.doc().text.get_untracked();
        let next = before.encode_utf16().nth(caret);
        if pairs && closers.contains(&caret) && next == Some(character as u16) {
            closers.retain(|at| *at != caret);
            caret += 1;
            continue;
        }
        let paired = if pairs {
            paired_text(&before, caret, caret, character, behavior)
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
        let edit = Edit::input_units(
            before,
            (caret, caret),
            after,
            (new_caret, new_caret),
            kind.into(),
            0.0,
        );
        if !state.commit(edit) {
            caret = state.caret_units();
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

/// Whether a character typed into `value` (written once in the text on
/// screen) is refused.
fn value_refuses_typing(state: EditorState, value: &str) -> bool {
    let text = state.doc().text.get_untracked();
    let Some(found) = text.find(value) else {
        return false;
    };
    let inside = found + value.len() / 2;
    let at = utf16(&text, inside);
    let mut typed = text.clone();
    typed.insert(inside, 'x');
    let edit = Edit::input_units(
        text,
        (at, at),
        typed,
        (at + 1, at + 1),
        "insertText".into(),
        0.0,
    );
    !state.commit(edit)
}

#[test]
fn a_second_copy_of_a_locked_key_is_taken_and_holds_the_save() {
    let mut failures = Vec::new();
    for case in CASES {
        for lock in [Lock::Path, Lock::Schema, Lock::Range] {
            // A host range keeps its text: where the language repeats an
            // element for a list (XML), a second copy is a new item.
            let repeats = case.language.behavior().repetition() != Repetition::None;
            for pairs in [false, true] {
                let (_owner, state) = mount(case, lock);
                let at = case.text.find(case.after).unwrap() + case.after.len();
                let typed = format!("\n{}{}", case.indent, case.entry);
                type_at(state, at, &typed, pairs);
                let now = state.doc().text.get_untracked();
                let wanted = format!("{}{typed}{}", &case.text[..at], &case.text[at..]);
                let label = format!("{} locked by {lock:?}, pairs {pairs}", case.language);
                if now != wanted {
                    failures.push(format!("{label}: not taken, left\n{now}"));
                    continue;
                }
                if state.save() && !(repeats && matches!(lock, Lock::Range)) {
                    failures.push(format!("{label}: saved with the duplicate\n{now}"));
                }
                if !value_refuses_typing(state, "hunter2") {
                    failures.push(format!("{label}: the locked value took typing"));
                }
            }
        }
    }
    report(failures);
}
