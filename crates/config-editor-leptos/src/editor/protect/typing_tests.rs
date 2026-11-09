//! Typing refused at a read-only field leaves nothing half-written: the
//! keys typed after the refused one are refused too (no stray spaces or
//! letters), and the word typing was writing when it was refused is taken
//! back (`<i` of a refused `<id>`); the refusal shows. In every language.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::FieldPath;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::spanless_tests::{ENV, TEXT as ENV_TEXT};
use super::tests::documents;

/// Types `typed` one character at a time at the caret `at` (UTF-16, ASCII
/// text here), as the browser's input events do.
fn type_at(state: EditorState, mut at: usize, typed: &str) {
    for character in typed.chars() {
        let before = state.doc().text.get_untracked();
        let text = format!("{}{character}{}", &before[..at], &before[at..]);
        let kind = if character == '\n' {
            "insertLineBreak"
        } else {
            "insertText"
        };
        let edit = Edit::input(before, (at, at), text, (at + 1, at + 1), kind.into(), 0.0);
        if state.commit(edit) {
            at += 1;
        } else {
            // A take-back moves the caret to where the word started.
            at = state.doc().caret.get_untracked();
        }
    }
}

fn without_whitespace(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn refused_typing_leaves_nothing_half_written() {
    // Per language: where a key is typed inside the locked `/meta` block,
    // the line opened first, and the entry typed.
    let cases = [
        (Language::YAML, "  b: 2", "\n  ", "extra: 5"),
        (Language::JSON, "\"meta\": {", "\n    ", "\"extra\": 5,"),
        (Language::TOML, "b = 2", "\n", "extra = 5"),
        (Language::HCL, "  b = 2", "\n  ", "extra = 5"),
        (Language::XML, "<b>2</b>", "\n  ", "<extra>5</extra>"),
    ];
    let mut failures = Vec::new();
    for (language, after, opened, typed) in cases {
        let text = documents()
            .into_iter()
            .find(|(known, _)| *known == language)
            .unwrap()
            .1;
        let mut editor = TestEditor::new(language, text);
        editor.read_only_paths = vec![FieldPath::from("/meta")];
        let (_owner, state) = editor.mount();
        let at = text.find(after).unwrap() + after.len();
        type_at(state, at, &format!("{opened}{typed}"));
        // Only the line opened stays: no letter, and no space typed after
        // the refused one.
        let now = state.doc().text.get_untracked();
        if now != format!("{}{opened}{}", &text[..at], &text[at..]) {
            failures.push(format!("{language}: left {now:?}"));
        }
        if state.protect().tip.get_untracked().is_none() {
            failures.push(format!("{language}: no refusal shown"));
        }
    }
    // A second locked `id` typed at the end: refused part-way, the part
    // typed before the refusal goes too.
    for (language, typed) in [
        (Language::YAML, "id: other\n"),
        (Language::HCL, "id = \"other\"\n"),
        (Language::XML, "<id>other</id>\n"),
    ] {
        let text = documents()
            .into_iter()
            .find(|(known, _)| *known == language)
            .unwrap()
            .1;
        let mut editor = TestEditor::new(language, text);
        editor.read_only_paths = vec![FieldPath::from("/id")];
        let (_owner, state) = editor.mount();
        type_at(state, text.len(), typed);
        let now = state.doc().text.get_untracked();
        if without_whitespace(&now) != without_whitespace(text) {
            failures.push(format!("{language} second id: left {now:?}"));
        }
    }
    // Typing elsewhere afterwards, or after moving, is not held back.
    let text = documents()[0].1;
    let mut editor = TestEditor::new(Language::YAML, text);
    editor.read_only_paths = vec![FieldPath::from("/meta")];
    let (_owner, state) = editor.mount();
    let at = text.find("  b: 2").unwrap() + 6;
    type_at(state, at, "\n  x");
    state.forget_refusal();
    let name = state.doc().text.get_untracked().find("name: x").unwrap() + 7;
    type_at(state, name, "yz");
    if !state.doc().text.get_untracked().contains("name: xyz") {
        failures.push(format!(
            "typing elsewhere held back: {:?}",
            state.doc().text.get_untracked()
        ));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// After a key glued onto a read-only value is refused, what is free next
/// to it is not held back by that refusal: a space, a comment after it, a
/// new line (Enter or Shift+Enter alike), at the same caret, and a comment
/// glued on where the language starts one right after a value.
#[test]
fn a_refused_key_does_not_hold_back_what_is_free_after_the_field() {
    // Where in each of `documents()` an X glues onto `/id` (after the text
    // here), a comment after it, and the comments glued on. Nothing glues
    // onto an XML element's end: there, the X renames the tag, and a space
    // after its name is free.
    let lines: [(&str, &str, &[&str]); 5] = [
        ("id: abc", " # c", &[]),
        ("\"id\": \"abc\"", " ", &[]),
        ("id = \"abc\"", " # c", &["#c"]),
        ("id = \"abc\"", " # c", &["#c", "//c", "/*c*/"]),
        ("<id", " ", &[]),
    ];
    let mut cases: Vec<(Language, &str, &str, &str, &[&str])> = documents()
        .into_iter()
        .zip(lines)
        .map(|((language, text), (line, comment, glued))| (language, text, line, comment, glued))
        .collect();
    cases.push((Language::new(&ENV), ENV_TEXT, "ID=srv-1", " ", &[]));
    let mut failures = Vec::new();
    for (language, text, line, comment, glued) in cases {
        let breaks: &[&str] = if line == "<id" { &[] } else { &["\n", " \n"] };
        for &next in [comment].iter().chain(breaks).chain(glued) {
            let mut editor = TestEditor::new(language, text);
            editor.read_only_paths = vec![FieldPath::from("/id"), FieldPath::from("/ID")];
            let (_owner, state) = editor.mount();
            let at = text.find(line).unwrap() + line.len();
            type_at(state, at, "X");
            if state.doc().text.get_untracked() != text {
                failures.push(format!(
                    "{language}: X glued onto the value was let through"
                ));
                continue;
            }
            type_at(state, at, next);
            let wanted = format!("{}{next}{}", &text[..at], &text[at..]);
            let now = state.doc().text.get_untracked();
            if now != wanted {
                failures.push(format!(
                    "{language}: {next:?} after the refused X left {now:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
