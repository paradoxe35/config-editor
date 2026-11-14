//! A new key typed in, or beside, a block holding a locked field, whose
//! name starts with the block's name (`dbname` in a `db` block, a TOML
//! `[db.extra]` or `[dbx]` beside a `[db]` table, `[clients_meta]` after
//! `[[clients]]`), passes on its way through a half-typed name equal to
//! the block's (`db`, `[db`, `"db"`, `[db]` with its `]` written by the
//! editor ahead of the caret): that is no second key holding the field
//! until written whole, and is never refused. A second key holding the
//! field, written whole, is still refused. In YAML, JSON, TOML, HCL and
//! XML, for secrets, read-only paths, the schema's `readOnly` and
//! read-only ranges, typed a key at a time with auto-closing on and off.
//! So is a new HCL block of a locked block's type with other labels
//! (`resource "aws_s3_bucket" "logs"` after a locked `resource
//! "aws_instance" "web"`), and a TOML table beside it (QA 37: with
//! auto-closing off, the line was wiped when its `{` was typed).

use leptos::prelude::*;

use super::super::Language;
use super::super::diagnostics::build_schema;
use super::super::edit::Edit;
use super::super::keyboard::{paired_edit, replace_selection};
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::save_matrix::report;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::utf16;

const TOKEN: &str = "«secret 0b1c»";

/// A document with a locked field (its value written `{v}`), the text new
/// entries are typed after, the new entries (each must be taken) and a
/// second key holding the field (refused, or left unparsed).
struct Case {
    label: &'static str,
    language: Language,
    text: &'static str,
    locked: &'static str,
    /// The locked field's line (one-based).
    line: usize,
    after: &'static str,
    entries: &'static [&'static str],
    duplicate: &'static str,
}

const CASES: &[Case] = &[
    Case {
        label: "YAML block",
        language: Language::YAML,
        text: "db:\n  user: app\n  password: {v}\nport: 1\n",
        locked: "/db/password",
        line: 3,
        after: "user: app",
        entries: &["\n  dbname: main", "\n  db_host: h", "\n  debug: x"],
        duplicate: "",
    },
    Case {
        label: "YAML top level",
        language: Language::YAML,
        text: "db:\n  user: app\n  password: {v}\nport: 1\n",
        locked: "/db/password",
        line: 3,
        after: "port: 1",
        entries: &["\ndbname: main", "\ndb_host: h"],
        duplicate: "\ndb:\n  password: y",
    },
    Case {
        label: "JSON object",
        language: Language::JSON,
        text: "{\n  \"db\": {\n    \"user\": \"app\",\n    \"password\": \"{v}\"\n  }\n}\n",
        locked: "/db/password",
        line: 4,
        after: "\"app\",",
        entries: &["\n    \"dbname\": \"main\",", "\n    \"db_host\": \"h\","],
        duplicate: "",
    },
    Case {
        label: "TOML table",
        language: Language::TOML,
        text: "[db]\nuser = \"app\"\npassword = \"{v}\"\n",
        locked: "/db/password",
        line: 3,
        after: "user = \"app\"",
        entries: &["\ndbname = \"main\"", "\ndb_host = \"h\""],
        duplicate: "",
    },
    Case {
        label: "TOML header",
        language: Language::TOML,
        text: "name = \"x\"\n\n[tls]\ncert = \"a.pem\"\nkey = \"{v}\"\n",
        locked: "/tls/key",
        line: 5,
        after: "key = \"{v}\"",
        entries: &["\n[tls.extra]", "\n[tlsx]", "\n[tls2.a]", "\n[server]"],
        duplicate: "\n[tls]",
    },
    Case {
        label: "TOML header holding a dotted table",
        language: Language::TOML,
        text: "[package]\nname = \"x\"\nversion = \"{v}\"\n",
        locked: "/package/version",
        line: 3,
        after: "version = \"{v}\"",
        entries: &["\n\n[package.metadata.docs]", "\n\n[packages]"],
        duplicate: "\n\n[package]",
    },
    Case {
        label: "TOML array of tables",
        language: Language::TOML,
        text: "[[clients]]\nname = \"a\"\nkey = \"{v}\"\n",
        locked: "/clients/0/key",
        line: 3,
        after: "key = \"{v}\"",
        entries: &["\n\n[clients_meta]", "\n\n[clients2]"],
        duplicate: "\n\n[clients]",
    },
    Case {
        label: "HCL block",
        language: Language::HCL,
        text: "db {\n  user = \"app\"\n  password = \"{v}\"\n}\n",
        locked: "/db/password",
        line: 3,
        after: "user = \"app\"",
        entries: &["\n  dbname = \"main\"", "\n  db_host = \"h\""],
        duplicate: "",
    },
    Case {
        label: "HCL top level",
        language: Language::HCL,
        text: "db {\n  user = \"app\"\n  password = \"{v}\"\n}\nport = 1\n",
        locked: "/db/password",
        line: 3,
        after: "port = 1",
        entries: &["\ndbname = \"main\"", "\ndb_host {\n}"],
        duplicate: "\ndb {\n  password = \"y\"\n}",
    },
    Case {
        label: "HCL labelled block",
        language: Language::HCL,
        text: "resource \"aws_instance\" \"web\" {\n  ami = \"{v}\"\n  instance_type = \"t3.micro\"\n}\n",
        locked: "/resource/aws_instance/web/ami",
        line: 2,
        after: "}",
        entries: &[
            "\nresource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"x\"\n}",
            "\nresource \"aws_instance\" \"db\" {\n  ami = \"ami-9\"\n}",
            "\nresource \"aws_instance\" \"web2\" {\n  ami = \"x\"\n}",
            "\ndata \"aws_instance\" \"web\" {\n}",
        ],
        duplicate: "\nresource \"aws_instance\" \"web\" {\n  ami = \"evil\"\n}",
    },
    Case {
        label: "HCL labelled block, typed inside its type",
        language: Language::HCL,
        text: "resource \"aws_instance\" \"web\" {\n  ami = \"{v}\"\n}\nresource \"aws_instance\" \"db\" {\n  ami = \"x\"\n}\n",
        locked: "/resource/aws_instance/web/ami",
        line: 2,
        after: "\"x\"",
        entries: &["\n  web {\n  }", "\n  tags = { web = 1 }"],
        duplicate: "",
    },
    Case {
        label: "TOML dotted header",
        language: Language::TOML,
        text: "[resource.aws_instance.web]\nami = \"{v}\"\n",
        locked: "/resource/aws_instance/web/ami",
        line: 2,
        after: "ami = \"{v}\"",
        entries: &[
            "\n\n[resource.aws_s3_bucket.logs]\nbucket = \"x\"",
            "\n\n[resource.aws_instance.db]\nami = \"x\"",
            "\n\n[resource.aws_instance.web2]",
        ],
        duplicate: "\n\n[resource.aws_instance.web]",
    },
    Case {
        label: "XML element",
        language: Language::XML,
        text: "<config>\n  <db>\n    <user>app</user>\n    <password>{v}</password>\n  </db>\n</config>\n",
        locked: "/config/db/password",
        line: 4,
        after: "<user>app</user>",
        entries: &["\n    <dbname>main</dbname>", "\n    <db_host>h</db_host>"],
        duplicate: "",
    },
];

#[derive(Clone, Copy, Debug)]
enum Lock {
    Secret,
    Path,
    Schema,
    Range,
}

/// A schema marking the field at `pointer` with `mark` (`"x-secret"` or
/// `"readOnly"`); a `0` segment is a list's items.
fn schema(pointer: &str, mark: &str) -> String {
    let mut schema = format!(r#"{{"type": "string", "{mark}": true}}"#);
    for key in pointer
        .split('/')
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        schema = if key == "0" {
            format!(r#"{{"type": "array", "items": {schema}}}"#)
        } else {
            format!(r#"{{"type": "object", "properties": {{"{key}": {schema}}}}}"#)
        };
    }
    schema
}

fn text_of(case: &Case, lock: Lock) -> String {
    let value = if matches!(lock, Lock::Secret) {
        TOKEN
    } else {
        "hunter2"
    };
    case.text.replace("{v}", value)
}

fn mount(case: &Case, lock: Lock) -> (Owner, EditorState) {
    let text = text_of(case, lock);
    let mut editor = TestEditor::new(case.language, &text);
    match lock {
        Lock::Secret => {
            editor.secrets = Some(SecretOptions::default());
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(case.locked, "x-secret")).unwrap(),
            ));
        }
        Lock::Path => editor.read_only_paths = vec![FieldPath::from(case.locked)],
        Lock::Schema => {
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(case.locked, "readOnly")).unwrap(),
            ));
        }
        Lock::Range => {
            let length = text.split('\n').nth(case.line - 1).unwrap().chars().count();
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
/// the caret, and typing that closer steps over it. Whether every key was
/// taken.
fn type_at(state: EditorState, at: usize, typed: &str, pairs: bool) -> bool {
    let behavior = state.behavior();
    let mut caret = utf16(&state.doc().text.get_untracked(), at);
    // Closers the editor wrote, at their UTF-16 offsets.
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
            return false;
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
    true
}

#[test]
fn a_new_key_starting_with_a_locked_blocks_name_is_taken() {
    let mut failures = Vec::new();
    for case in CASES {
        for lock in [Lock::Secret, Lock::Path, Lock::Schema, Lock::Range] {
            for pairs in [false, true] {
                for entry in case.entries {
                    let (_owner, state) = mount(case, lock);
                    let text = state.doc().text.get_untracked();
                    let after = case.after.replace(
                        "{v}",
                        if matches!(lock, Lock::Secret) {
                            TOKEN
                        } else {
                            "hunter2"
                        },
                    );
                    let at = text.find(&after).unwrap() + after.len();
                    let taken = type_at(state, at, entry, pairs);
                    let now = state.doc().text.get_untracked();
                    if !taken || !now.contains(entry) {
                        failures.push(format!(
                            "{} locked by {lock:?}, pairs {pairs}: typing {entry:?} refused:\n{now}",
                            case.label
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}

#[test]
fn a_second_key_holding_a_locked_field_is_still_refused() {
    let mut failures = Vec::new();
    for case in CASES.iter().filter(|case| !case.duplicate.is_empty()) {
        // A secret is no lock on its value (a new value may replace the
        // placeholder): only read-only fields refuse a second holder.
        for lock in [Lock::Path, Lock::Schema, Lock::Range] {
            for pairs in [false, true] {
                let (_owner, state) = mount(case, lock);
                let text = state.doc().text.get_untracked();
                let after = case.after.replace(
                    "{v}",
                    if matches!(lock, Lock::Secret) {
                        TOKEN
                    } else {
                        "hunter2"
                    },
                );
                let at = text.find(&after).unwrap() + after.len();
                type_at(state, at, case.duplicate, pairs);
                let now = state.doc().text.get_untracked();
                let added =
                    now.matches(case.duplicate).count() > text.matches(case.duplicate).count();
                if added && case.language.parse(&now).is_ok() {
                    failures.push(format!(
                        "{} locked by {lock:?}, pairs {pairs}: a second holder taken:\n{now}",
                        case.label
                    ));
                }
            }
        }
    }
    report(failures);
}
