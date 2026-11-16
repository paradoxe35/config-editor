//! P2: a value the user did not touch keeps its exact host text, also
//! after views that write it otherwise (an XML view reads every scalar
//! as text, and a switch converting unsaved edits writes the next view
//! from that reading); and the items an edit leaves alone keep the link
//! to their host source that tells it.

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;

fn mount(host: Language, view: Language, text: &str) -> (Owner, EditorState, RwSignal<String>) {
    let saved = RwSignal::new(text.to_string());
    let mut editor = TestEditor::new(host, text);
    editor.edit_as = Some(view);
    editor.on_change = Some(Callback::new(move |text| saved.set(text)));
    let (owner, state) = editor.mount();
    (owner, state, saved)
}

/// Types `to` over the first `from` of the text on screen.
fn type_over(state: EditorState, from: &str, to: &str) {
    let before = state.doc().text.get_untracked();
    let at = before
        .find(from)
        .unwrap_or_else(|| panic!("{from:?} in {before}"));
    let units = |text: &str| text.encode_utf16().count();
    let start = units(&before[..at]);
    let end = start + units(from);
    let text = format!("{}{to}{}", &before[..at], &before[at + from.len()..]);
    let caret = start + units(to);
    state.commit(Edit::input_units(
        before,
        (start, end),
        text,
        (caret, caret),
        "insertText".to_string(),
        0.0,
    ));
}

fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

/// An unsaved XML view switched to HCL writes `8080` (the XML text read
/// as a number) and `3`: saved later, the host's `'8080'` and `!!str 3`
/// stay strings, written as they were.
#[test]
fn a_value_left_alone_keeps_its_host_text_through_an_unsaved_switch() {
    let host = "count: 5\nport: '8080'\nlevel: !!str 3\nname: web\n";
    let (_owner, state, saved) = mount(Language::YAML, Language::XML, host);
    // A number emptied in a view of text: the save waits.
    type_over(state, ">5<", "><");
    pause(state);
    assert!(
        !state.sync().in_sync.get_untracked(),
        "the save waits: {}",
        saved.get_untracked()
    );
    switch_to(state, Language::HCL);
    assert_eq!(state.sync().edit_language.get_untracked(), Language::HCL);
    // The number typed again, and another value changed.
    let shown = state.doc().text.get_untracked();
    let emptied = shown
        .lines()
        .find(|line| line.starts_with("count"))
        .unwrap_or_else(|| panic!("count in\n{shown}"))
        .to_string();
    type_over(state, &emptied, "count = 6");
    type_over(state, "web", "app");
    pause(state);
    let shown = state.doc().text.get_untracked();
    assert!(state.sync().in_sync.get_untracked(), "saved:\n{shown}");
    assert_eq!(
        saved.get_untracked(),
        "count: 6\nport: '8080'\nlevel: !!str 3\nname: app\n",
        "the view:\n{shown}"
    );
}

/// P3: a Replace all whose replacement adds a line break says each match
/// it replaced, so the items between two matches keep their links (read
/// as one stretch from the first match to the last, the edit cut every
/// item in between from its host source).
#[cfg(feature = "search")]
#[test]
fn a_replace_all_adding_lines_keeps_the_links_of_the_items_between() {
    let host = "{\n  \"a\": {\"x\": 1},\n  \"b\": {\"y\": 2},\n  \"c\": {\"z\": 1}\n}\n";
    let (_owner, state, _saved) = mount(Language::JSON, Language::YAML, host);
    state.search().query.set(": 1".into());
    state.search().replacement.set(": 1\n  n: 0".into());
    super::super::search::replace_all(state);
    let text = state.doc().text.get_untracked();
    assert!(text.contains("n: 0"), "replaced:\n{text}");
    let mut links = state.sync().links.get_value();
    let told = links
        .told(Language::YAML.behavior(), &text, host, true)
        .expect("the links tell");
    for pointer in ["/b", "/b/y"] {
        assert!(
            told.items.contains_key(pointer),
            "{pointer} lost its link: {:?}\n{text}",
            told.items.keys().collect::<Vec<_>>()
        );
    }
}
