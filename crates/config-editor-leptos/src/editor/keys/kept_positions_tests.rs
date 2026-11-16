//! What the editor keeps by position across edits (the Tab stops and
//! typed closers an accepted suggestion leaves, closed folds) follows each
//! edit's change: an edit elsewhere, before or after them, leaves them on
//! their text.

use std::sync::Arc;

use leptos::prelude::*;

use super::super::completion::completions;
use super::super::edit::{Change, Edit};
use super::super::fold_ranges::FoldRange;
use super::super::languages::Language;
use super::super::schema::build_schema;
use super::super::state::EditorState;
use super::super::sync::take_host_value;
use super::super::test_state::TestEditor;
use super::tab::next_placeholder_slot;

/// An editor on `text` in `language` with `schema`, the suggestion
/// `label` accepted at UTF-16 `at`.
fn accepted(
    language: Language,
    schema: &str,
    text: &str,
    at: usize,
    label: &str,
) -> (Owner, EditorState) {
    let bundle = Arc::new(build_schema(schema).expect("the schema"));
    let options = completions(text, at, true, Some(&bundle), true, language.behavior());
    let option = options
        .iter()
        .find(|option| option.label == label)
        .unwrap_or_else(|| panic!("{label} offered"))
        .clone();
    let mut editor = TestEditor::new(language, text);
    editor.schema = Some(bundle);
    let (owner, state) = editor.mount();
    state.accept(&option, false);
    (owner, state)
}

const SERVICE: &str = r#"{"type":"object","properties":{"svc":{"type":"object",
    "required":["name","port"],
    "properties":{
        "name":{"type":"string","default":"web"},
        "port":{"type":"integer","default":8080}
    }}}}"#;

/// The UTF-16 range of `word`'s first copy in `text`.
fn range_of(text: &str, word: &str) -> (usize, usize) {
    let at = text.find(word).expect("in the text");
    let start = text[..at].encode_utf16().count();
    (start, start + word.encode_utf16().count())
}

/// The values Tab selects in turn in the block an accept wrote
/// (`svc:\n  name: web\n  port: 8080\n`, the caret at `web`), in the text
/// as it is after `host` (the host's new value, made of the text after the
/// accept).
fn tab_after_host_value(host: impl Fn(&str) -> String) -> (String, Vec<(usize, usize)>) {
    let (_owner, state) = accepted(Language::YAML, SERVICE, "", 0, "svc");
    let written = state.doc().text.get_untracked();
    assert_eq!(written, "svc:\n  name: web\n  port: 8080\n");
    take_host_value(state, host(&written));
    let text = state.doc().text.get_untracked();
    let mut selected = Vec::new();
    while selected.len() < 3 && next_placeholder_slot(state, &text, state.caret_units()) {
        selected.extend(state.completion().placeholder.get_value());
    }
    (text, selected)
}

#[test]
fn a_host_value_after_an_accepted_block_leaves_its_tab_stops_on_their_values() {
    let (text, selected) = tab_after_host_value(|written| format!("{written}other: 1\n"));
    assert_eq!(
        selected,
        [range_of(&text, "web"), range_of(&text, "8080")],
        "{text:?}"
    );
}

#[test]
fn a_host_value_before_an_accepted_block_moves_its_tab_stops_with_it() {
    let (text, selected) = tab_after_host_value(|written| format!("first: 0\n{written}"));
    assert_eq!(
        selected,
        [range_of(&text, "web"), range_of(&text, "8080")],
        "{text:?}"
    );
}

#[test]
fn a_host_value_after_a_typed_closer_leaves_it_to_type_over() {
    let schema = r#"{"type":"object","properties":{"name":{"type":"string"}}}"#;
    let text = "{\n  \n}\n";
    let (_owner, state) = accepted(Language::JSON, schema, text, 4, "name");
    let written = state.doc().text.get_untracked();
    let caret = state.caret_units();
    assert!(state.closer_at(caret).is_some(), "{written:?} at {caret}");
    take_host_value(state, format!("{written}\n"));
    assert!(
        state.closer_at(caret).is_some(),
        "the closing quote at {caret}"
    );
}

/// A character typed before a folded line's last character, the same as
/// it, is typed before the line's end, which leaves the fold closed (read
/// off the two texts alone, it would be typed at the end, which opens it).
#[test]
fn typing_before_a_folded_lines_end_keeps_it_closed_whatever_the_character() {
    let text = "a:\n  b: 1\nc: # n\n  d: 2\n";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    let fold = FoldRange::new(2, 3);
    assert!(state.foldable_untracked().contains(&fold));
    state.fold().folded.set(vec![fold]);
    // `c: # |n` + `n`.
    let at = text.find("n\n").expect("the line's end");
    let change = Change::insert_units(text, at, "n");
    assert!(state.commit(Edit::command_units(text.to_string(), change, (at, at))));
    assert_eq!(
        state.doc().text.get_untracked(),
        "a:\n  b: 1\nc: # nn\n  d: 2\n"
    );
    assert_eq!(state.fold().folded.get_untracked(), [fold]);
}
