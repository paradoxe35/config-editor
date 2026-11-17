//! The editor keeps the edits made to an Edit-as view between saves, and
//! each typing pause's save reads which item is which from them: an item
//! cut and pasted after a pause keeps its `xsi:type`, and the
//! first of two items with the same data deleted takes its own away, as typed in the editor.

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
    state.commit(Edit::new_units(before, (caret, caret), text, caret));
}

fn paste(state: EditorState, inserted: &str, before_line: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(before_line).expect("the line");
    let at = before[..at].rfind('\n').map_or(0, |line| line + 1);
    let text = format!("{}{inserted}{}", &before[..at], &before[at..]);
    let caret = before[..at].encode_utf16().count() + inserted.encode_utf16().count();
    let start = before[..at].encode_utf16().count();
    state.commit(Edit::new_units(before, (start, start), text, caret));
}

fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
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
    state.commit(Edit::new_units(before, (caret, caret), next, caret));
    (owner, state)
}

/// An item deleted in a view, then `edit_as` switched before
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

/// A view whose links are not known (none kept) tells no save its items:
/// the items' values do, so none takes another's `xsi:type` by place.
#[test]
fn a_view_with_no_links_tells_no_save() {
    let sizes = [1u32, 2, 3];
    for deleted in [0usize, 1] {
        let (_owner, state) = deleted_in(Language::YAML, &sizes, deleted);
        state
            .sync()
            .links
            .set_value(super::super::bridge::links::Links::default());
        pause(state);
        let saved = state.sync().host_text.get_value();
        assert_eq!(saved, shapes(&left(&sizes, Some(deleted))));
    }
}

/// A switch converting unsaved edits (an item deleted) keeps each item's
/// link on the item at the same pointer of the next view: the save after
/// it keeps each remaining item's own `xsi:type`.
#[test]
fn a_switch_converting_unsaved_edits_keeps_the_links() {
    for sizes in [[1u32, 2, 3], [1, 1, 1]] {
        let (_owner, state) = deleted_in(Language::YAML, &sizes, 0);
        let before = state.doc().text.get_untracked();
        let value = Language::YAML
            .behavior()
            .parse(&before)
            .expect("the view reads");
        let converted = format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("written")
        );
        let switch = Edit::switching(before, converted, Language::JSON, Default::default(), None);
        state.commit(switch);
        pause(state);
        let saved = state.sync().host_text.get_value();
        assert_eq!(saved, shapes(&left(&sizes, Some(0))), "{sizes:?}");
    }
}

/// An item deleted and saved, `edit_as` switched, then the
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

/// A controlled host sets its `value` to each `on_change` text at once:
/// the echo of an edit typed in the host's language is no new session, so
/// an undo back across the switch still saves as the host text the view
/// was saved as (the switch's step records it).
#[test]
fn a_controlled_echo_keeps_the_host_text_an_undo_back_saves_as() {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::XML, &shapes(&left(&[1, 2], None)));
    editor.edit_as = Some(Language::YAML);
    editor.on_change = Some(Callback::new(move |text: String| {
        heard.update(|heard| heard.push(text));
    }));
    let (_owner, state) = editor.mount();
    let echo = |state: EditorState| {
        for text in heard.try_update(std::mem::take).unwrap_or_default() {
            super::super::sync::take_host_value(state, text);
        }
    };
    let before = state.doc().text.get_untracked();
    let (next, caret) = without_item(Language::YAML, &before, 0);
    state.commit(Edit::new_units(before, (caret, caret), next, caret));
    pause(state);
    echo(state);
    let saved = state.sync().host_text.get_value();
    assert_eq!(saved, shapes(&left(&[1, 2], Some(0))));
    switch_to(state, Language::XML);
    let before = state.doc().text.get_untracked();
    let typed = before.replacen("</shapes>", "  <shape />\n</shapes>", 1);
    let end = before.encode_utf16().count();
    state.commit(Edit::new_units(before, (end, end), typed, end));
    echo(state);
    for _ in 0..2 {
        let undo = state.history_edit(false).expect("a step to undo");
        state.commit(undo);
        echo(state);
    }
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
    pause(state);
    echo(state);
    assert!(state.sync().in_sync.get_untracked(), "the save waits");
    assert_eq!(state.sync().host_text.get_value(), saved);
}
