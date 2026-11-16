//! A new field typed beside a locked one whose name starts with, or holds,
//! the locked field's name (`<namespace>` beside a locked `<name>`,
//! `[tls2]` beside a locked `[tls]`) is a new field: typed a key at a
//! time, it passes through a half-typed name equal to the locked one
//! (`<name`, `[tls`), which is no field yet, and is never refused. A
//! second copy of the locked field itself is taken too and never saved. For locks
//! by read-only path, by the schema's `readOnly` and by read-only range;
//! in XML (elements and attributes) and TOML (tables), with YAML, JSON
//! and HCL as controls.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::language::Repetition;
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::save_matrix::report;
use super::super::schema::build_schema;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::utf16;

/// A document with a locked field, where new text goes, and the new
/// entry for a field name.
struct Case {
    label: &'static str,
    language: Language,
    text: &'static str,
    /// The locked field: its pointer, and its line (one-based) and the
    /// columns its read-only range takes (`None`: the whole line).
    locked: &'static str,
    line: usize,
    columns: Option<(usize, usize)>,
    /// The locked field's name.
    name: &'static str,
    /// The text the new entry is typed after.
    after: &'static str,
    entry: fn(&str) -> String,
}

const CASES: [Case; 8] = [
    Case {
        label: "XML element",
        language: Language::XML,
        text: "<config>\n  <app>\n    <name>edge</name>\n    <port>80</port>\n  </app>\n</config>\n",
        locked: "/config/app/name",
        line: 3,
        columns: None,
        name: "name",
        after: "<port>80</port>",
        entry: |name| format!("\n    <{name}>x</{name}>"),
    },
    Case {
        label: "XML element before the locked one",
        language: Language::XML,
        text: "<config>\n  <app>\n    <port>80</port>\n    <name>edge</name>\n  </app>\n</config>\n",
        locked: "/config/app/name",
        line: 4,
        columns: None,
        name: "name",
        after: "<port>80</port>",
        entry: |name| format!("\n    <{name}>x</{name}>"),
    },
    Case {
        label: "XML attribute",
        language: Language::XML,
        text: "<server id=\"srv-42\" name=\"edge\">\n  <port>80</port>\n</server>\n",
        locked: "/server/id",
        line: 1,
        columns: Some((9, 20)),
        name: "id",
        after: "\"edge\"",
        entry: |name| format!(" {name}=\"5\""),
    },
    Case {
        label: "TOML table",
        language: Language::TOML,
        text: "name = \"x\"\n\n[limits]\nmax = 10\n\n[tls]\ncert = \"a.pem\"\n",
        locked: "/limits",
        line: 3,
        columns: None,
        name: "limits",
        after: "cert = \"a.pem\"",
        entry: |name| format!("\n\n[{name}]"),
    },
    Case {
        label: "TOML table holding a locked field",
        language: Language::TOML,
        text: "name = \"x\"\n\n[limits]\nmax = 10\n\n[tls]\ncert = \"a.pem\"\n",
        locked: "/tls/cert",
        line: 7,
        columns: None,
        name: "tls",
        after: "cert = \"a.pem\"",
        entry: |name| format!("\n\n[{name}]"),
    },
    Case {
        label: "YAML key",
        language: Language::YAML,
        text: "app:\n  name: edge\n  port: 80\n",
        locked: "/app/name",
        line: 2,
        columns: None,
        name: "name",
        after: "port: 80",
        entry: |name| format!("\n  {name}: x"),
    },
    Case {
        label: "JSON key",
        language: Language::JSON,
        text: "{\n  \"app\": {\n    \"name\": \"edge\",\n    \"port\": 80\n  }\n}\n",
        locked: "/app/name",
        line: 3,
        columns: None,
        name: "name",
        after: "\"port\": 80",
        entry: |name| format!(",\n    \"{name}\": \"x\""),
    },
    Case {
        label: "HCL attribute",
        language: Language::HCL,
        text: "app {\n  name = \"edge\"\n  port = 80\n}\n",
        locked: "/app/name",
        line: 2,
        columns: None,
        name: "name",
        after: "port = 80",
        entry: |name| format!("\n  {name} = \"x\""),
    },
];

#[derive(Clone, Copy, Debug)]
enum Lock {
    Path,
    Schema,
    Range,
}

/// A schema marking the field at `pointer` `readOnly`.
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
            let (from, to) = case.columns.unwrap_or((1, length + 1));
            editor.read_only_ranges = vec![TextRange {
                start: TextPosition {
                    line: case.line,
                    column: from,
                },
                end: TextPosition {
                    line: case.line,
                    column: to,
                },
            }];
        }
    }
    editor.on_save = Some(Callback::new(
        |_: super::super::host_editing::SaveRequest| {},
    ));
    editor.mount()
}

/// Types `typed` at byte `at`, a key at a time, as the browser's input
/// events do; whether every key was taken.
fn type_at(state: EditorState, mut at: usize, typed: &str) -> bool {
    for character in typed.chars() {
        let before = state.doc().text.get_untracked();
        let text = format!("{}{character}{}", &before[..at], &before[at..]);
        let start = utf16(&before, at);
        let kind = if character == '\n' {
            "insertLineBreak"
        } else {
            "insertText"
        };
        let caret = start + character.len_utf16();
        let edit = Edit::input_units(
            before,
            (start, start),
            text,
            (caret, caret),
            kind.into(),
            0.0,
        );
        if !state.commit(edit) {
            return false;
        }
        at += character.len_utf8();
    }
    true
}

#[test]
fn a_new_field_named_after_a_locked_one_is_not_the_locked_field() {
    let mut failures = Vec::new();
    for case in &CASES {
        // Starting with the locked name, holding it, and a second copy of
        // the locked field (never saved: it is the locked field written
        // twice).
        let names = [
            (format!("{}space", case.name), true),
            (format!("{}2", case.name), true),
            (format!("x{}", case.name), true),
            (format!("re{}d", case.name), true),
            (case.name.to_string(), false),
        ];
        for lock in [Lock::Path, Lock::Schema, Lock::Range] {
            // A host range keeps its text: where the language repeats an
            // element for a list (XML), a second copy is a new item.
            let repeats = case.language.behavior().repetition() != Repetition::None
                && matches!(lock, Lock::Range);
            // A table's lock by schema is the table's own (`readOnly` on an
            // object), its range its header line.
            for (name, free) in &names {
                let (_owner, state) = mount(case, lock);
                let text = state.doc().text.get_untracked();
                let at = text.find(case.after).unwrap() + case.after.len();
                let entry = (case.entry)(name);
                let taken = type_at(state, at, &entry);
                let now = state.doc().text.get_untracked();
                let written = now.matches(&entry).count() > case.text.matches(&entry).count();
                if *free && !(taken && written) {
                    failures.push(format!(
                        "{} locked by {lock:?}: typing {entry:?} refused:\n{now}",
                        case.label
                    ));
                }
                if !*free && written && !repeats && state.save() {
                    failures.push(format!(
                        "{} locked by {lock:?}: a second {name} saved:\n{now}",
                        case.label
                    ));
                }
            }
        }
    }
    report(failures);
}
