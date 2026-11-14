//! The editor keeps the edits made to an Edit-as view between saves, and
//! each typing pause's save reads which item is which from them: an item
//! cut and pasted after a pause keeps its `xsi:type` (QA `E32-2`), and the
//! first of two items with the same data deleted takes its own away (QA
//! `E32-3`), as typed in the editor.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use super::save::xml_item_edits::without_item;
use leptos::prelude::*;

const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

fn mount(text: &str) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::XML, text);
    editor.edit_as = Some(Language::YAML);
    editor.mount()
}

/// The view's text with `removed` (whole lines) cut, the caret left where
/// they were; or with `inserted` put at the start of the line holding
/// `before`, the caret after it.
fn cut(state: EditorState, removed: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(removed).expect("the lines");
    let text = before.replacen(removed, "", 1);
    let caret = before[..at].encode_utf16().count();
    state.commit(Edit::new(before, (caret, caret), text, caret));
}

fn paste(state: EditorState, inserted: &str, before_line: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(before_line).expect("the line");
    let at = before[..at].rfind('\n').map_or(0, |line| line + 1);
    let text = format!("{}{inserted}{}", &before[..at], &before[at..]);
    let caret = before[..at].encode_utf16().count() + inserted.encode_utf16().count();
    let start = before[..at].encode_utf16().count();
    state.commit(Edit::new(before, (start, start), text, caret));
}

fn pause(state: EditorState) {
    convert_now(state, state.doc().text.get_untracked(), Language::YAML);
}

#[test]
fn an_item_cut_and_pasted_after_a_pause_keeps_its_markup() {
    let host = format!(
        "<nlog {XSI}>\n  <targets>\n    <target xsi:type=\"File\" name=\"logfile\" />\n    <target xsi:type=\"Console\" name=\"console\" />\n    <target xsi:type=\"Null\" name=\"blackhole\" />\n  </targets>\n</nlog>\n"
    );
    let (_owner, state) = mount(&host);
    let line = state
        .doc()
        .text
        .get_untracked()
        .lines()
        .find(|line| line.contains("name: console"))
        .expect("the console line")
        .to_string()
        + "\n";
    cut(state, &line);
    pause(state);
    paste(state, &line, "name: logfile");
    pause(state);
    let saved = state.sync().host_text.get_value();
    let console = saved.find("name=\"console\"").expect("saved");
    let logfile = saved.find("name=\"logfile\"").expect("saved");
    assert!(console < logfile, "{saved}");
    assert!(
        saved.contains("<target xsi:type=\"Console\" name=\"console\" />"),
        "{saved}"
    );
}

#[test]
fn deleting_the_first_of_two_alike_items_takes_its_markup_away() {
    let host = format!(
        "<shapes {XSI}>\n  <shape xsi:type=\"Circle\">\n    <size>1</size>\n  </shape>\n  <shape xsi:type=\"Square\">\n    <size>1</size>\n  </shape>\n</shapes>\n"
    );
    let (_owner, state) = mount(&host);
    let line = state
        .doc()
        .text
        .get_untracked()
        .lines()
        .find(|line| line.contains("size: 1"))
        .expect("an item line")
        .to_string()
        + "\n";
    cut(state, &line);
    pause(state);
    let saved = state.sync().host_text.get_value();
    assert!(
        saved.contains("Square") && !saved.contains("Circle"),
        "{saved}"
    );
}

/// The host of shapes, the item at `i` typed `T{i}`, sized as listed.
fn shapes(sizes: &[(usize, u32)]) -> String {
    let mut text = format!("<shapes {XSI}>\n");
    for (index, size) in sizes {
        text.push_str(&format!(
            "  <shape xsi:type=\"T{index}\">\n    <size>{size}</size>\n  </shape>\n"
        ));
    }
    text + "</shapes>\n"
}

/// `sizes`, each with its index, less the item `deleted`.
fn left(sizes: &[u32], deleted: Option<usize>) -> Vec<(usize, u32)> {
    sizes
        .iter()
        .copied()
        .enumerate()
        .filter(|(at, _)| Some(*at) != deleted)
        .collect()
}

/// An editor of the shapes `sizes` as `view`, item `deleted` deleted.
fn deleted_in(view: Language, sizes: &[u32], deleted: usize) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::XML, &shapes(&left(sizes, None)));
    editor.edit_as = Some(view);
    let (owner, state) = editor.mount();
    let before = state.doc().text.get_untracked();
    let (next, caret) = without_item(view, &before, deleted);
    state.commit(Edit::new(before, (caret, caret), next, caret));
    (owner, state)
}

/// QA `E33-1`: an item deleted in a view, then `edit_as` switched before
/// the typing pause saved the edit: the next save keeps each remaining
/// item's own `xsi:type`, in every view and switched to every other one
/// (the host's language too), the items' data alike or not.
#[test]
fn an_item_deleted_then_switched_away_before_the_pause_keeps_each_items_markup() {
    let targets = [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::XML,
    ];
    let mut failures = Vec::new();
    for sizes in [[1u32, 2, 3], [1, 1, 1]] {
        for view in [Language::YAML, Language::JSON] {
            for deleted in [0usize, 1] {
                for target in targets.into_iter().filter(|target| *target != view) {
                    let (_owner, state) = deleted_in(view, &sizes, deleted);
                    switch_to(state, target);
                    convert_now(state, state.doc().text.get_untracked(), target);
                    let saved = state.sync().host_text.get_value();
                    if saved != shapes(&left(&sizes, Some(deleted))) {
                        failures.push(format!(
                            "{sizes:?} as {view:?}, delete {deleted}, to {target:?}:\n{saved}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Edits followed from a text the host was never saved as (a trail a
/// switch began on unsaved edits) tell no save its items: the items'
/// values do, so none takes another's `xsi:type` by place.
#[test]
fn a_trail_begun_on_an_unsaved_text_tells_no_save() {
    let sizes = [1u32, 2, 3];
    for deleted in [0usize, 1] {
        let (_owner, state) = deleted_in(Language::YAML, &sizes, deleted);
        let now = state.doc().text.get_untracked();
        state
            .sync()
            .trail
            .set_value(super::super::bridge::item_trail::Trail::starting(&now));
        pause(state);
        let saved = state.sync().host_text.get_value();
        assert_eq!(saved, shapes(&left(&sizes, Some(deleted))));
    }
}

/// A switch whose save of the unsaved edits waits converts them into the
/// next view: the trail then knows nothing of their items.
#[test]
fn a_switch_converting_unsaved_edits_drops_the_trail() {
    let mut editor = TestEditor::new(Language::XML, &shapes(&left(&[1, 2], None)));
    editor.edit_as = Some(Language::YAML);
    let (_owner, state) = editor.mount();
    let converted = "shapes:\n  shape:\n    - size: 2\n".to_string();
    // As the switch commits it, the host's text not saved.
    let before = state.doc().text.get_untracked();
    let switch = Edit::switching(before, converted.clone(), Language::JSON, (0, 0), None);
    state.commit(switch);
    assert!(
        !state
            .sync()
            .trail
            .with_value(|trail| trail.begins_on(&converted)),
        "the trail begins on a text never saved"
    );
}

/// QA `E33-4`: an item deleted and saved, `edit_as` switched, then the
/// switch undone: the view's text is the one saved before, so it saves as
/// the host text it was saved as, not converted again (a list of one read
/// back otherwise than the lone element the host holds, and the save
/// waited).
#[test]
fn a_switch_undone_back_to_a_saved_text_saves_as_it() {
    for target in [Language::JSON, Language::TOML, Language::XML] {
        let (_owner, state) = deleted_in(Language::YAML, &[1, 2], 0);
        pause(state);
        let saved = state.sync().host_text.get_value();
        assert_eq!(saved, shapes(&left(&[1, 2], Some(0))));
        switch_to(state, target);
        let undo = state.history_edit(false).expect("the switch to undo");
        state.commit(undo);
        assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
        pause(state);
        assert!(
            state.sync().in_sync.get_untracked(),
            "to {target:?}: the save waits"
        );
        assert_eq!(state.sync().host_text.get_value(), saved, "to {target:?}");
    }
}
