//! A lock guards its own field only: in the block holding a locked field,
//! a sibling typed on a new line (before or after the lock, on a blank
//! line, with a flow value, an XML comment), and a sibling's separator
//! deleted and typed again, are free in every language, however the field
//! is locked (a read-only path, the schema's `readOnly`, a host range over
//! the value or its line). The locked field still reads as before, and
//! still refuses typing; a second key of the locked field typed there is
//! still refused.
//!
//! The first characters of a new line leave the text unparseable; they
//! used to be refused as if they joined the locked field.

use leptos::prelude::*;

use super::super::Language;
use super::super::codec;
use super::super::edit::Edit;
use super::super::state::EditorState;
use super::taken_tests::{KINDS_OF_LEAF, Sample, mount, sample, typed_into, units};

/// Per language: the indent of the block's entries, a new entry, one with
/// a flow (or nested) value, the leaf's key written as a new entry, and
/// the separator after a sibling's key.
struct Writing {
    indent: &'static str,
    entry: &'static str,
    nested: &'static str,
    duplicate: &'static str,
    separator: &'static str,
}

fn writing(language: Language) -> Writing {
    match language.label() {
        "YAML" => Writing {
            indent: "    ",
            entry: "ca: 1",
            nested: "ca: {a: 1}",
            duplicate: "max_pending: 1",
            separator: "burst:",
        },
        "JSON" => Writing {
            indent: "      ",
            entry: "\"ca\": 1,",
            nested: "\"ca\": {\"a\": 1},",
            duplicate: "\"max_pending\": 1,",
            separator: "\"burst\":",
        },
        "TOML" => Writing {
            indent: "",
            entry: "ca = 1",
            nested: "ca = {a = 1}",
            duplicate: "max_pending = 1",
            separator: "burst =",
        },
        "HCL" => Writing {
            indent: "    ",
            entry: "ca = 1",
            nested: "ca = {a = 1}",
            duplicate: "max_pending = 1",
            separator: "burst =",
        },
        _ => Writing {
            indent: "      ",
            entry: "<ca>1</ca>",
            nested: "<!-- c -->",
            duplicate: "<max_pending>1</max_pending>",
            separator: "<burst>",
        },
    }
}

/// Types `typed` at byte `at` (ASCII here), one key at a time; false at
/// the first key refused.
fn type_at(state: EditorState, mut at: usize, typed: &str) -> bool {
    for character in typed.chars() {
        if !insert(state, at, &character.to_string()) {
            return false;
        }
        at += 1;
    }
    true
}

/// Inserts `with` at byte `at` as one input event.
fn insert(state: EditorState, at: usize, with: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..at], &before[at..]);
    let caret = units(&before, at) + with.encode_utf16().count();
    let kind = if with.starts_with('\n') {
        "insertLineBreak"
    } else {
        "insertText"
    };
    let start = units(&before, at);
    let edit = Edit::input(
        before,
        (start, start),
        after,
        (caret, caret),
        kind.into(),
        0.0,
    );
    state.commit(edit)
}

/// Deletes the byte before `at` (Backspace).
fn backspace(state: EditorState, at: usize) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{}", &before[..at - 1], &before[at..]);
    let start = units(&before, at - 1);
    let edit = Edit::input(
        before,
        (start + 1, start + 1),
        after,
        (start, start),
        "deleteContentBackward".into(),
        0.0,
    );
    state.commit(edit)
}

/// The end of the line holding `line` (whole, its indent included).
fn line_end(text: &str, line: &str) -> usize {
    let at = text
        .find(&format!("\n{line}\n"))
        .unwrap_or_else(|| panic!("no line {line:?}"));
    at + 1 + line.len()
}

/// The sibling lines a new entry is typed after: the block's header, the
/// line before the lock, the lock's own line.
fn anchors(sample: &Sample) -> [&'static str; 3] {
    let before = sample
        .text
        .lines()
        .find(|line| line.contains("burst"))
        .expect("the sibling before the lock");
    let before = &sample.text[sample.text.find(before).unwrap()..][..before.len()];
    [sample.header_line, before, sample.leaf_line]
}

/// The locked value still reads as before (when the text parses) and
/// still refuses typing.
fn lock_holds(state: EditorState, language: Language, sample: &Sample) -> Option<String> {
    let now = state.doc().text.get_untracked();
    let document = codec::read(language.behavior(), sample.text, None).expect("parses");
    if let Ok(read) = codec::read(language.behavior(), &now, None)
        && read.pointer(sample.leaf) != document.pointer(sample.leaf)
    {
        return Some(format!("the lock reads {:?}", read.pointer(sample.leaf)));
    }
    let taken = typed_into(state);
    (!taken.is_empty()).then(|| format!("typing taken at {taken:?}"))
}

#[test]
fn a_sibling_of_a_locked_field_is_typed_freely() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        let sample = sample(language);
        let writing = writing(language);
        for kind in KINDS_OF_LEAF {
            let mut fail = |what: String| {
                failures.push(format!("{} {kind:?}: {what}", language.label()));
            };
            for anchor in anchors(&sample) {
                for (blank, entry) in [
                    (false, writing.entry),
                    (true, writing.entry),
                    (false, writing.nested),
                ] {
                    let (_owner, state) = mount(language, kind);
                    let at = line_end(sample.text, anchor);
                    let typed = if blank {
                        // A blank line first, the indent typed by hand.
                        insert(state, at, "\n") && type_at(state, at + 1, writing.indent)
                    } else {
                        insert(state, at, &format!("\n{}", writing.indent))
                    };
                    let start = at + 1 + writing.indent.len();
                    let now = state.doc().text.get_untracked();
                    if !typed || !type_at(state, start, entry) {
                        fail(format!(
                            "{entry:?} after {anchor:?} refused (blank {blank}) at {now:?}"
                        ));
                        continue;
                    }
                    if let Some(why) = lock_holds(state, language, &sample) {
                        fail(format!("{entry:?} after {anchor:?}: {why}"));
                    }
                }
            }
            // A sibling's separator deleted, then typed again.
            let (_owner, state) = mount(language, kind);
            let at =
                sample.text.find(writing.separator).expect("the sibling") + writing.separator.len();
            let last = &writing.separator[writing.separator.len() - 1..];
            if !backspace(state, at) || !insert(state, at - 1, last) {
                fail(format!("{last:?} of the sibling refused"));
            } else if let Some(why) = lock_holds(state, language, &sample) {
                fail(format!("separator: {why}"));
            }
            // The locked key typed again beside it stays refused.
            let (_owner, state) = mount(language, kind);
            let at = line_end(sample.text, anchors(&sample)[1]);
            insert(state, at, &format!("\n{}", writing.indent));
            type_at(state, at + 1 + writing.indent.len(), writing.duplicate);
            let now = state.doc().text.get_untracked();
            if now.contains(writing.duplicate) {
                fail(format!("a second locked key let through: {now:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
