//! A secret element's tag renamed in XML: its name is written twice (the
//! start tag and the end tag), so the text does not parse between the two
//! renames. As a key renamed in the other languages, the rename is free,
//! a key at a time, whichever tag goes first, by typing over the name, by
//! deleting letters, or by typing beside them (after the first
//! letter, the rest was refused as "nothing can be written beside it").
//! The token stays as it is.

use leptos::prelude::*;

use super::super::Language;
use super::super::save_matrix::report;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::many_copies_tests::shown;
use super::xml_list_secret_tests::{retype, write};

const MASK: &str = "«secret 0b1c»";

fn xml() -> String {
    format!(
        "<config>\n  <b>\n    <user>u</user>\n    <key>{MASK}</key>\n    <x>1</x>\n  </b>\n</config>\n"
    )
}

fn mount(host: Language) -> (Owner, EditorState) {
    let text = if host == Language::XML {
        xml()
    } else {
        let value = Language::XML.parse(&xml()).expect("XML parses");
        host.serialize(&value).expect("written in the host")
    };
    let mut editor = TestEditor::new(host, &text);
    if host != Language::XML {
        editor.edit_as = Some(Language::XML);
    }
    editor.secrets = Some(SecretOptions::default());
    editor.mount()
}

/// Deletes the letters of the `nth` `needle` (past its first `skip`
/// bytes) one at a time from the end (Backspace), then types `with`.
fn backspace_and_type(state: EditorState, (needle, skip): (&str, usize), with: &str) -> bool {
    let text = shown(state);
    let Some((at, _)) = text.match_indices(needle).next() else {
        return false;
    };
    let (from, mut to) = (at + skip, at + needle.len());
    while to > from {
        if !write(state, to - 1..to, "", false) {
            return false;
        }
        to -= 1;
    }
    write(state, from..from, with, true)
}

/// Types `with` right after the first `skip` bytes of `needle`.
fn type_into(state: EditorState, (needle, skip): (&str, usize), with: &str) -> bool {
    let text = shown(state);
    let Some((at, _)) = text.match_indices(needle).next() else {
        return false;
    };
    write(state, at + skip..at + skip, with, true)
}

type Step = (&'static str, fn(EditorState) -> bool);

const WAYS: [(&str, [Step; 2], &str); 4] = [
    (
        "typed over, start tag first",
        [
            ("start tag", |state| retype(state, ("<key", 1), 0, "other")),
            ("end tag", |state| retype(state, ("</key", 2), 0, "other")),
        ],
        "other",
    ),
    (
        "typed over, end tag first",
        [
            ("end tag", |state| retype(state, ("</key", 2), 0, "other")),
            ("start tag", |state| retype(state, ("<key", 1), 0, "other")),
        ],
        "other",
    ),
    (
        "letters deleted, then typed",
        [
            ("start tag", |state| {
                backspace_and_type(state, ("<key", 2), "id")
            }),
            ("end tag", |state| {
                backspace_and_type(state, ("</key", 3), "id")
            }),
        ],
        "kid",
    ),
    (
        "typed before the name",
        [
            ("end tag", |state| type_into(state, ("</key", 2), "api")),
            ("start tag", |state| type_into(state, ("<key", 1), "api")),
        ],
        "apikey",
    ),
];

#[test]
fn a_secret_elements_tags_rename_freely() {
    let mut failures = Vec::new();
    for host in [Language::XML, Language::YAML, Language::JSON] {
        for (way, steps, name) in WAYS {
            let (_owner, state) = mount(host);
            for (step, act) in steps {
                if !act(state) {
                    failures.push(format!(
                        "{host}, {way}: {step} refused at\n{}",
                        shown(state)
                    ));
                }
            }
            let renamed = format!("<{name}>{MASK}</{name}>");
            if !shown(state).contains(&renamed) {
                failures.push(format!("{host}, {way}: not {renamed}:\n{}", shown(state)));
            }
        }
    }
    report(failures);
}
