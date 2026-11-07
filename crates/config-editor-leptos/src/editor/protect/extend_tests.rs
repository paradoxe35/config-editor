//! A locked field's value can never grow: no key, table, element or
//! duplicate added inside or next to it, whether typed a character at a
//! time (through texts that do not parse) or pasted whole, in every
//! language and while editing as another.

use serde_json::Value;

use super::super::Language;
use super::super::codec;
use super::super::edit::Edit;
use super::super::paths::FieldPath;
use super::super::test_state::TestEditor;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::tests::{documents, schema};

/// Where to insert what, in each language's document of `documents()`:
/// `(what, after this text (or at the start with ""), inserted)`.
fn additions(language: Language) -> Vec<(&'static str, &'static str, &'static str)> {
    match language.label() {
        "YAML" => vec![
            ("a key in the locked block", "  b: 2", "\n  c: 3"),
            ("a duplicate locked key", "port: 80\n", "id: x\n"),
            ("a second locked block", "port: 80\n", "meta:\n  c: 3\n"),
            ("a key in a flow copy", "port: 80\n", "meta: {c: 3}\n"),
        ],
        "JSON" => vec![
            ("a key in the locked block", "\"b\": 2", ",\n    \"c\": 3"),
            ("a duplicate locked key", "{\n", "  \"id\": \"x\",\n"),
            ("a second locked block", "{\n", "  \"meta\": {},\n"),
            (
                "a duplicate at the end",
                "\"port\": 80",
                ",\n  \"id\": \"x\"",
            ),
        ],
        "TOML" => vec![
            ("a key in the locked table", "b = 2\n", "c = 3\n"),
            ("a duplicate locked key", "", "id = \"x\"\n"),
            ("a second locked table", "b = 2\n", "[meta]\nc = 3\n"),
            (
                "a sub-table of the locked table",
                "b = 2\n",
                "[meta.sub]\nx = 1\n",
            ),
            ("a dotted key into the locked table", "", "meta.c = 3\n"),
        ],
        "HCL" => vec![
            ("a key in the locked block", "  b = 2\n", "  c = 3\n"),
            ("a duplicate locked key", "port = 80\n", "id = \"x\"\n"),
            (
                "a second locked block",
                "port = 80\n",
                "meta {\n  c = 3\n}\n",
            ),
        ],
        _ => vec![
            (
                "an element in the locked block",
                "  <b>2</b>\n",
                "  <c>3</c>\n",
            ),
            (
                "a duplicate locked element",
                "<port>80</port>\n",
                "<id>x</id>\n",
            ),
            (
                "a second locked block",
                "<port>80</port>\n",
                "<meta><c>3</c></meta>\n",
            ),
        ],
    }
}

/// The locks of `text`, carried from `previous` as the editor's memo does.
fn locks_after(language: Language, text: &str, previous: Option<&Protected>) -> Protected {
    let schema = schema();
    protect(
        language.behavior(),
        text,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            previous,
            ..Sources::default()
        },
    )
}

/// `inserted` put into `text` at byte `at` in `steps`, each edit passing
/// the guard as the editor runs it; the text left, and whether every step
/// went through.
fn insert(language: Language, text: &str, at: usize, steps: &[&str]) -> (String, bool) {
    let schema = schema();
    let mut text = text.to_string();
    let mut protected = locks_after(language, &text, None);
    let mut at = at;
    let mut all = true;
    for step in steps {
        let edited = format!("{}{step}{}", &text[..at], &text[at..]);
        if check(
            language.behavior(),
            Some(&schema),
            &protected,
            &text,
            &edited,
        )
        .is_err()
        {
            all = false;
            continue;
        }
        protected = locks_after(language, &edited, Some(&protected));
        text = edited;
        at += step.len();
    }
    (text, all)
}

fn chars(text: &str) -> Vec<&str> {
    text.char_indices()
        .map(|(at, character)| &text[at..at + character.len_utf8()])
        .collect()
}

fn locked_values(language: Language, text: &str) -> Option<(Value, Value)> {
    let document = codec::read(language.behavior(), text, Some(&schema())).ok()?;
    Some((
        document.pointer("/id")?.clone(),
        document.pointer("/meta")?.clone(),
    ))
}

#[test]
fn nothing_is_added_to_a_locked_field_typed_or_pasted_in_any_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let original = locked_values(language, text).expect("the document reads");
        for (what, after, inserted) in additions(language) {
            let Some(found) = text.find(after) else {
                failures.push(format!("{language}: {what}: {after:?} not found"));
                continue;
            };
            let at = found + after.len();
            for (how, steps) in [("typed", chars(inserted)), ("pasted", vec![inserted])] {
                let (left, all) = insert(language, text, at, &steps);
                if all {
                    failures.push(format!("{language}: {what} {how}: allowed\n{left}"));
                }
                if let Some(values) = locked_values(language, &left)
                    && values != original
                {
                    failures.push(format!(
                        "{language}: {what} {how}: locked values became {values:?}\n{left}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The same additions typed into a live editor whose host locks `/meta`
/// and `/id` by path, on each host language and editing as each other.
#[test]
fn a_live_editor_refuses_extending_a_locked_block_in_every_edit_as_language() {
    use leptos::prelude::*;
    let mut failures = Vec::new();
    for (host, host_text) in documents() {
        for (edit, _) in documents() {
            let (_owner, state) = TestEditor {
                edit_as: (edit != host).then_some(edit),
                read_only_paths: vec![FieldPath::from("/id"), FieldPath::from("/meta")],
                ..TestEditor::new(host, host_text)
            }
            .mount();
            let shown = state.doc().text.get_untracked();
            let original = codec::read(edit.behavior(), &shown, None).ok();
            for (what, after, inserted) in additions(edit) {
                take_host_shown(state, &shown);
                let Some(found) = shown.find(after) else {
                    // Another language writes it differently: the guard
                    // matrix covers the language itself.
                    continue;
                };
                let mut at = found + after.len();
                let mut all = true;
                for step in chars(inserted) {
                    let before = state.doc().text.get_untracked();
                    let edited = format!("{}{step}{}", &before[..at], &before[at..]);
                    let caret = at + step.len();
                    if state.commit(Edit::new(before, (at, at), edited, caret)) {
                        at = caret;
                    } else {
                        all = false;
                    }
                }
                let left = state.doc().text.get_untracked();
                let read = codec::read(edit.behavior(), &left, None).ok();
                let changed = read.is_some_and(|read| {
                    let original = original.as_ref().expect("the shown text reads");
                    ["/id", "/meta"]
                        .iter()
                        .any(|pointer| read.pointer(pointer) != original.pointer(pointer))
                });
                if all || changed {
                    failures.push(format!("{host} as {edit}: {what}: allowed\n{left}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Puts the shown text back as a new text from the host would.
fn take_host_shown(state: super::super::state::EditorState, shown: &str) {
    use leptos::prelude::*;
    let before = state.doc().text.get_untracked();
    if before != shown {
        let mut edit = Edit::new(before, (0, 0), shown.to_string(), 0);
        edit.origin = super::super::edit::Origin::Switch;
        state.commit(edit);
    }
}
