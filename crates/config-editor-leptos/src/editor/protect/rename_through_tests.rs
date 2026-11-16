//! A key or an HCL block label renamed in place, a key at a time, whose
//! new name passes through the name of a block holding a locked field
//! (`cache` retyped as `db_backup` beside a locked `db/password`, a label
//! `"db"` retyped as `"webapp"` beside a locked `"web"`, `dynamic
//! "egress"` as `"ingress_v2"` beside a locked `dynamic "ingress"`), or
//! through the locked field's own name: the name on its way is no second
//! key holding the field, and is never refused (QA 38: the run was
//! refused at `db`, leaving `d:`). A rename ending on the locked name
//! itself is a duplicate: taken (no span's text changes), and never saved
//! while it is there. In YAML, JSON, TOML, HCL and XML, for secrets,
//! read-only paths, the schema's `readOnly` and read-only ranges, with
//! auto-closing on and off.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::language::Repetition;
use super::super::save_matrix::report;
use super::super::state::EditorState;
use super::guarantee_tests::utf16;
use super::holder_prefix_tests::{Lock, TOKEN, mount_locked, type_at};

/// A document with a locked field (its value written `{v}`), the name
/// renamed (found in the last copy of `around`), the new names typed over
/// it (each must be taken) and the locked name it may not end on.
struct Case {
    label: &'static str,
    language: Language,
    text: &'static str,
    locked: &'static str,
    /// The locked field's line (one-based).
    line: usize,
    around: &'static str,
    name: &'static str,
    names: &'static [&'static str],
    duplicate: &'static str,
}

const CASES: &[Case] = &[
    Case {
        label: "YAML key",
        language: Language::YAML,
        text: "db:\n  password: {v}\ncache:\n  host: x\n",
        locked: "/db/password",
        line: 2,
        around: "cache:",
        name: "cache",
        names: &["db_backup", "dbx", "db-2", "d"],
        duplicate: "db",
    },
    Case {
        label: "YAML sibling of the field",
        language: Language::YAML,
        text: "db:\n  password: {v}\n  user: app\n",
        locked: "/db/password",
        line: 2,
        around: "user:",
        name: "user",
        names: &["password_old", "passwords"],
        duplicate: "password",
    },
    Case {
        label: "JSON key",
        language: Language::JSON,
        text: "{\n  \"db\": {\n    \"password\": \"{v}\"\n  },\n  \"cache\": {\n    \"host\": \"x\"\n  }\n}\n",
        locked: "/db/password",
        line: 3,
        around: "\"cache\"",
        name: "cache",
        names: &["db_backup", "dbx"],
        duplicate: "db",
    },
    Case {
        label: "JSON sibling of the field",
        language: Language::JSON,
        text: "{\n  \"db\": {\n    \"password\": \"{v}\",\n    \"user\": \"app\"\n  }\n}\n",
        locked: "/db/password",
        line: 3,
        around: "\"user\"",
        name: "user",
        names: &["password_old"],
        duplicate: "password",
    },
    Case {
        label: "TOML table",
        language: Language::TOML,
        text: "[db]\npassword = \"{v}\"\n\n[cache]\nhost = \"x\"\n",
        locked: "/db/password",
        line: 2,
        around: "[cache]",
        name: "cache",
        names: &["db_backup", "dbx"],
        duplicate: "db",
    },
    Case {
        label: "HCL block",
        language: Language::HCL,
        text: "db {\n  password = \"{v}\"\n}\n\ncache {\n  host = \"x\"\n}\n",
        locked: "/db/password",
        line: 2,
        around: "cache {",
        name: "cache",
        names: &["db_backup", "dbx"],
        duplicate: "db",
    },
    Case {
        label: "HCL sibling of the field",
        language: Language::HCL,
        text: "db {\n  password = \"{v}\"\n  user = \"app\"\n}\n",
        locked: "/db/password",
        line: 2,
        around: "user =",
        name: "user",
        names: &["password_old"],
        duplicate: "password",
    },
    Case {
        label: "HCL block label",
        language: Language::HCL,
        text: "resource \"aws_instance\" \"web\" {\n  ami = \"{v}\"\n}\n\nresource \"aws_instance\" \"db\" {\n  ami = \"ami-9\"\n}\n",
        locked: "/resource/aws_instance/web/ami",
        line: 2,
        around: "\"db\"",
        name: "db",
        names: &["webapp", "web2", "w"],
        duplicate: "web",
    },
    Case {
        label: "HCL dynamic block label",
        language: Language::HCL,
        text: "resource \"aws_security_group\" \"sg\" {\n  dynamic \"ingress\" {\n    content {\n      token = \"{v}\"\n    }\n  }\n  dynamic \"egress\" {\n    content {\n      port = 1\n    }\n  }\n}\n",
        locked: "/resource/aws_security_group/sg/dynamic/ingress/content/token",
        line: 4,
        around: "\"egress\"",
        name: "egress",
        names: &["ingress_v2", "ingressx"],
        duplicate: "ingress",
    },
    Case {
        label: "XML start tag",
        language: Language::XML,
        text: "<config>\n  <db>\n    <password>{v}</password>\n  </db>\n  <cache>\n    <host>x</host>\n  </cache>\n</config>\n",
        locked: "/config/db/password",
        line: 3,
        around: "<cache>",
        name: "cache",
        names: &["db_backup", "dbx"],
        duplicate: "db",
    },
];

fn text_of(case: &Case, lock: Lock) -> String {
    let value = if matches!(lock, Lock::Secret) {
        TOKEN
    } else {
        "hunter2"
    };
    case.text.replace("{v}", value)
}

/// Selects the case's name and types `typed` over it a key at a time, as
/// the editor's keys do: the first key replaces the selection. Whether
/// every key was taken.
fn rename(state: EditorState, case: &Case, typed: &str, pairs: bool) -> bool {
    let before = state.doc().text.get_untracked();
    let start = before.rfind(case.around).unwrap() + case.around.find(case.name).unwrap();
    let end = start + case.name.len();
    let mut keys = typed.chars();
    let first = keys.next().unwrap();
    let after = format!("{}{first}{}", &before[..start], &before[end..]);
    let (from, to) = (utf16(&before, start), utf16(&before, end));
    let edit = Edit::input_units(
        before,
        (from, to),
        after,
        (from + 1, from + 1),
        "insertText".into(),
        0.0,
    );
    state.commit(edit) && type_at(state, start + first.len_utf8(), keys.as_str(), pairs)
}

/// The name as the text writes it once renamed to `name`.
fn renamed(case: &Case, name: &str) -> String {
    case.around.replacen(case.name, name, 1)
}

#[test]
fn a_name_retyped_through_a_locked_blocks_name_is_taken() {
    let mut failures = Vec::new();
    for case in CASES {
        for lock in [Lock::Secret, Lock::Path, Lock::Schema, Lock::Range] {
            for pairs in [false, true] {
                for name in case.names {
                    let (_owner, state) = mount_locked(
                        case.language,
                        &text_of(case, lock),
                        case.locked,
                        case.line,
                        lock,
                    );
                    let taken = rename(state, case, name, pairs);
                    let now = state.doc().text.get_untracked();
                    if !taken || !now.contains(&renamed(case, name)) {
                        failures.push(format!(
                            "{} locked by {lock:?}, pairs {pairs}: renaming to {name:?} refused:\n{now}",
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
fn a_name_retyped_to_the_locked_name_is_never_saved() {
    let mut failures = Vec::new();
    for case in CASES {
        // A secret is no lock on its value (a new value may replace the
        // placeholder): only read-only fields refuse a second holder.
        for lock in [Lock::Path, Lock::Schema, Lock::Range] {
            // A host range keeps its text: in a language repeating a block
            // for a list, a second block of its name is a new item, and the
            // range's value reads on there (saved as any edit).
            if matches!(lock, Lock::Range)
                && case.language.behavior().repetition() != Repetition::None
            {
                continue;
            }
            for pairs in [false, true] {
                let (_owner, state) = mount_locked(
                    case.language,
                    &text_of(case, lock),
                    case.locked,
                    case.line,
                    lock,
                );
                rename(state, case, case.duplicate, pairs);
                let now = state.doc().text.get_untracked();
                let written = renamed(case, case.duplicate);
                if now.contains(&written) && state.save() {
                    failures.push(format!(
                        "{} locked by {lock:?}, pairs {pairs}: a second holder taken and saved:\n{now}",
                        case.label
                    ));
                }
            }
        }
    }
    report(failures);
}
