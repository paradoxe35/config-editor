//! QA `E37-1` (low): TOML keys moved together from a table into an item
//! of an array of tables keep their forms (a datetime, a hex
//! number), and the item gains no blank line.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::moved_forms::{key_line, line_offset, pause, units};
use crate::editor::save_matrix::report;
use leptos::prelude::*;

const HOST: &str = "[a]\nwhen = 1979-05-27T07:32:00Z\nmask = 0xff\nk = 1\n\n[[srv]]\nname = \"s1\"\n\n[[srv]]\nname = \"s2\"\n";

const MOVED: &str = "[a]\nk = 1\n\n[[srv]]\nname = \"s1\"\nwhen = 1979-05-27T07:32:00Z\nmask = 0xff\n\n[[srv]]\nname = \"s2\"\n";

/// The keys `when` and `mask` cut from the YAML view and pasted
/// into the first server, as written or (`reindented`) then indented.
fn moved(pausing: bool, reindented: bool) -> Result<(), String> {
    let mut editor = TestEditor::new(Language::TOML, HOST);
    editor.edit_as = Some(Language::YAML);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let (from, to) = (
        line_offset(&text, key_line(&text, "when")),
        line_offset(&text, key_line(&text, "k")),
    );
    let lines = text[from..to].to_string();
    let after = format!("{}{}", &text[..from], &text[to..]);
    let caret = units(&text[..from]);
    state.commit(Edit::new(
        text.clone(),
        (caret, caret + units(&lines)),
        after,
        caret,
    ));
    if pausing {
        pause(state);
    }
    let text = state.doc().text.get_untracked();
    let at = line_offset(&text, key_line(&text, "- name") + 1);
    let deeper: String = lines
        .split_inclusive('\n')
        .map(|line| format!("  {line}"))
        .collect();
    let paste = |text: &str, at: usize, removed: usize, inserted: &str| {
        let after = format!("{}{inserted}{}", &text[..at], &text[at + removed..]);
        let start = units(&text[..at]);
        state.commit(Edit::new(
            text.to_string(),
            (start, start + units(&text[at..at + removed])),
            after,
            start + units(inserted),
        ));
    };
    if reindented {
        paste(&text, at, 0, &lines);
        let text = state.doc().text.get_untracked();
        paste(&text, at, lines.len(), &deeper);
    } else {
        paste(&text, at, 0, &deeper);
    }
    pause(state);
    let saved = state.sync().host_text.get_value();
    if !state.sync().in_sync.get_untracked() {
        return Err(format!(
            "the save waits ({:?}); view:\n{}",
            state.sync().conversion_problems.get_untracked(),
            state.doc().text.get_untracked()
        ));
    }
    if saved != MOVED {
        return Err(format!("saved:\n{saved}"));
    }
    Ok(())
}

#[test]
fn keys_moved_into_an_array_item_keep_their_forms() {
    let mut failures = Vec::new();
    for pausing in [false, true] {
        for reindented in [false, true] {
            if let Err(error) = moved(pausing, reindented) {
                failures.push(format!("pause {pausing}, reindented {reindented}: {error}"));
            }
        }
    }
    report(failures);
}
