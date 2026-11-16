//! Typing at a read-only field is judged key by key, by the spans alone:
//! a key typed inside a locked block's text is refused (the refusal
//! shows), one typed past its end changes no span and is taken, the save
//! then waiting while the block reads otherwise; a second locked key typed
//! at the end is taken the same way. Nothing is taken back: typing
//! elsewhere afterwards is never held. In every language.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::host_editing::SaveRequest;
use super::super::paths::FieldPath;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::tests::documents;

/// Types `typed` one character at a time at the caret `at` (UTF-16, ASCII
/// text here), as the browser's input events do; a refused key leaves the
/// caret where it was.
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
        }
    }
}

fn mount(language: Language, text: &str, paths: &[&str]) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(language, text);
    editor.read_only_paths = paths.iter().map(|path| FieldPath::from(*path)).collect();
    editor.on_save = Some(Callback::new(|_: SaveRequest| {}));
    editor.mount()
}

fn document(language: Language) -> &'static str {
    documents()
        .into_iter()
        .find(|(known, _)| *known == language)
        .unwrap()
        .1
}

#[test]
fn typing_at_a_locked_block_is_refused_in_it_and_holds_the_save_past_it() {
    // Per language: where a key is typed at the locked `/meta` block, the
    // line opened first, the entry typed, and whether that is inside the
    // block's text (refused) or past its end (taken, the save waiting).
    let cases = [
        (Language::YAML, "  b: 2", "\n  ", "extra: 5", false),
        (
            Language::JSON,
            "\"meta\": {",
            "\n    ",
            "\"extra\": 5,",
            true,
        ),
        (Language::TOML, "b = 2", "\n", "extra = 5", false),
        (Language::HCL, "  b = 2", "\n  ", "extra = 5", true),
        (Language::XML, "<b>2</b>", "\n  ", "<extra>5</extra>", true),
    ];
    let mut failures = Vec::new();
    for (language, after, opened, typed, inside) in cases {
        let text = document(language);
        let (_owner, state) = mount(language, text, &["/meta"]);
        let at = text.find(after).unwrap() + after.len();
        type_at(state, at, &format!("{opened}{typed}"));
        let now = state.doc().text.get_untracked();
        let refused = format!("{}{opened}{}", &text[..at], &text[at..]);
        let taken = format!("{}{opened}{typed}{}", &text[..at], &text[at..]);
        if inside {
            // Whitespace between its tokens is free inside the block.
            let flat = |text: &str| text.split_whitespace().collect::<String>();
            if flat(&now) != flat(&refused) {
                failures.push(format!("{language}: inside the block, left {now:?}"));
            }
            if state.protect().tip().is_none() {
                failures.push(format!("{language}: no refusal shown"));
            }
        } else {
            if now != taken {
                failures.push(format!("{language}: past the block, left {now:?}"));
            }
            if state.save() {
                failures.push(format!("{language}: saved with the block extended"));
            }
        }
    }
    // A second locked `id` typed at the end: taken, the save waiting.
    for (language, typed) in [
        (Language::YAML, "id: other\n"),
        (Language::HCL, "id = \"other\"\n"),
        (Language::XML, "<id>other</id>\n"),
    ] {
        let text = document(language);
        let (_owner, state) = mount(language, text, &["/id"]);
        type_at(state, text.len(), typed);
        let now = state.doc().text.get_untracked();
        if now != format!("{text}{typed}") {
            failures.push(format!("{language} second id: left {now:?}"));
        }
        if state.save() {
            failures.push(format!("{language} second id: saved"));
        }
    }
    // Typing elsewhere afterwards is not held back.
    let text = document(Language::YAML);
    let (_owner, state) = mount(Language::YAML, text, &["/meta"]);
    let at = text.find("  b: 2").unwrap() + 6;
    type_at(state, at, "\n  x");
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
    let cases: Vec<(Language, &str, &str, &str, &[&str])> = documents()
        .into_iter()
        .zip(lines)
        .map(|((language, text), (line, comment, glued))| (language, text, line, comment, glued))
        .collect();
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
