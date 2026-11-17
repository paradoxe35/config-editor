//! A list item copied in an Edit-as view (Ctrl+C, and
//! pasted in the same list) keeps the form its host writes it in, as a
//! copied key does: an XML element's `xsi:type`, a YAML tag, a TOML hex
//! number; with a pause between the copy and the paste or not.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::moved_forms::{pause, units};
use super::moved_forms_again::copy;
use super::save::xml_item_edits::item_lines;
use crate::editor::save_matrix::report;
use leptos::prelude::*;

const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

/// A host of a list, the item copied, and the text the save keeps of it.
struct Case {
    host: Language,
    text: String,
    views: &'static [Language],
    kept: &'static str,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            host: Language::TOML,
            text: "l = [\n  0x1F,\n  2,\n]\n".into(),
            views: &[Language::JSON, Language::YAML],
            kept: "0x1F",
        },
        Case {
            host: Language::YAML,
            text: "r:\n  l:\n    - !Ref a\n    - b\n".into(),
            views: &[Language::JSON, Language::XML],
            kept: "!Ref a",
        },
        Case {
            host: Language::XML,
            text: format!(
                "<nlog {XSI}>\n  <targets>\n    <target xsi:type=\"File\" name=\"f\" />\n    <target xsi:type=\"Console\" name=\"c\" />\n  </targets>\n</nlog>\n"
            ),
            views: &[Language::YAML, Language::JSON],
            kept: "xsi:type=\"File\"",
        },
    ]
}

/// The byte ranges of the first two list items' lines in `view`'s text.
fn first_items(view: Language, text: &str) -> Vec<(usize, usize)> {
    let offset = |line: usize| text.split_inclusive('\n').take(line).map(str::len).sum();
    let lines = item_lines(view, text);
    if lines.len() >= 2 {
        return lines
            .iter()
            .map(|(from, to)| (offset(*from), offset(*to)))
            .collect();
    }
    // One item per line, at the list's indent (a JSON or XML list of
    // scalars).
    let all: Vec<&str> = text.split_inclusive('\n').collect();
    let item = |line: &str| {
        let line = line.trim();
        line.starts_with("<l>")
            || !line.is_empty()
                && !line.contains(':')
                && !line.starts_with(['[', ']', '{', '}', '<'])
    };
    (0..all.len())
        .filter(|at| item(all[*at]))
        .map(|at| (offset(at), offset(at + 1)))
        .collect()
}

fn copied(case: &Case, view: Language, pausing: bool) -> Result<(), String> {
    let mut editor = TestEditor::new(case.host, &case.text);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let items = first_items(view, &text);
    let [(from, to), (next, _), ..] = items.as_slice() else {
        return Err(format!("no two items in\n{text}"));
    };
    let lines = text[*from..*to].to_string();
    copy(state, (*from, *to));
    if pausing {
        pause(state);
    }
    // Pasted before the second item.
    let before = state.doc().text.get_untracked();
    let after = format!("{}{lines}{}", &before[..*next], &before[*next..]);
    let start = units(&before[..*next]);
    state.commit(Edit::new_units(
        before,
        (start, start),
        after,
        start + units(&lines),
    ));
    pause(state);
    let saved = state.sync().host_text.get_value();
    if !state.sync().in_sync.get_untracked() {
        return Err(format!(
            "the save waits ({:?}); view:\n{}",
            state.sync().conversion_problems.get_untracked(),
            state.doc().text.get_untracked()
        ));
    }
    if saved.matches(case.kept).count() != 2 {
        return Err(format!(
            "saved:\n{saved}\nview:\n{}",
            state.doc().text.get_untracked()
        ));
    }
    Ok(())
}

/// The editors of a documents view share one clipboard per host language:
/// an item copied in one document's view and pasted in another's keeps the
/// form its host wrote it in (P3), as a paste in the same document does.
#[test]
fn an_item_copied_into_another_document_keeps_its_hosts_form() {
    let clipboard = StoredValue::new(crate::editor::bridge::links::Clipboard::default());
    let mount = |text: &str| {
        let mut editor = TestEditor::new(Language::TOML, text);
        editor.edit_as = Some(Language::JSON);
        editor.clipboard = Some(clipboard);
        editor.mount()
    };
    let (_first_owner, first) = mount("l = [\n  0x1F,\n  2,\n]\n");
    let (_second_owner, second) = mount("m = [\n  3,\n]\n");
    let text = first.doc().text.get_untracked();
    let (from, to) = first_items(Language::JSON, &text)[0];
    let line = text[from..to].to_string();
    copy(first, (from, to));
    let before = second.doc().text.get_untracked();
    let (at, _) = first_items(Language::JSON, &before)[0];
    let after = format!("{}{line}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    second.commit(Edit::new_units(
        before,
        (start, start),
        after,
        start + units(&line),
    ));
    pause(second);
    assert!(second.sync().in_sync.get_untracked(), "the save waits");
    let saved = second.sync().host_text.get_value();
    assert!(saved.contains("0x1F"), "{saved}");
}

#[test]
fn a_list_item_copied_keeps_its_hosts_form() {
    let mut failures = Vec::new();
    for case in cases() {
        for view in case.views {
            for pausing in [false, true] {
                if let Err(error) = copied(&case, *view, pausing) {
                    failures.push(format!(
                        "{:?} as {view:?}, pause {pausing}: {error}",
                        case.host
                    ));
                }
            }
        }
    }
    report(failures);
}
