//! Saves from an Edit-as view: a value the host's language cannot carry
//! exactly waits (round 20's `D20.docs-1`), and a formatting-only edit,
//! which leaves the host's text as it was, leaves the document clean once
//! converted (round 17's `D17.ux-1`). In every host and view.

#![cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::save_matrix::{ALL, report};
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::SaveRequest;

/// What the host heard: `on_change` and `on_save`.
struct Heard {
    changes: RwSignal<Vec<String>>,
    saves: RwSignal<Vec<String>>,
}

fn mount(host: Language, edit: Language, text: &str) -> (Owner, EditorState, Heard) {
    let heard = Heard {
        changes: RwSignal::new(Vec::new()),
        saves: RwSignal::new(Vec::new()),
    };
    let (changes, saves) = (heard.changes, heard.saves);
    let mut editor = TestEditor::new(host, text);
    editor.edit_as = Some(edit);
    editor.on_change = Some(Callback::new(move |text| {
        changes.update(|all| all.push(text))
    }));
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        saves.update(|all| all.push(request.text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

fn replace_all(state: EditorState, text: &str) {
    let before = state.doc().text.get_untracked();
    let end = before.encode_utf16().count();
    let caret = text.encode_utf16().count();
    state.commit(Edit::new(before, (0, end), text.to_string(), caret));
}

fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

/// Each edit of `/s` the host may not read back as the view holds it: a
/// one-item list, an empty list, an empty map, a list of lists, a list of
/// an empty map, an empty string.
fn values() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!(["b"]),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!([["b", "c"], ["d"]]),
        serde_json::json!([{}]),
        serde_json::json!(""),
    ]
}

/// A save, the typing pause's conversion or a flush either writes a host
/// text that reads back as the view holds the value, or waits: `in_sync`
/// false, `on_change` silent, `save()` false with an error problem.
#[test]
fn a_value_the_host_cannot_read_back_as_the_view_holds_it_waits() {
    let mut failures = Vec::new();
    for host in ALL {
        let host_text = host
            .serialize(&serde_json::json!({"r": {"s": "a", "f": 1}}))
            .unwrap();
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            for value in values() {
                let case = format!("{host} host as {edit}, /r/s = {value}");
                let (_owner, state, heard) = mount(host, edit, &host_text);
                let shown = state.doc().text.get_untracked();
                let Ok(mut read) = edit.parse(&shown) else {
                    failures.push(format!("{case}: the view does not parse"));
                    continue;
                };
                read["r"]["s"] = value.clone();
                let Ok(typed) =
                    super::super::codec::serialize(edit.behavior(), &read, Some(&shown))
                else {
                    continue;
                };
                // A value the view itself cannot hold is no host's loss; nor
                // is an empty map in a view that writes it as an empty
                // element, which reads as text over the host's string.
                if edit.parse(&typed).ok().as_ref() != Some(&read)
                    || edit.behavior().scalars() == crate::editor::language::Scalars::Text
                        && value == serde_json::json!({})
                {
                    continue;
                }
                replace_all(state, &typed);
                pause(state);
                let in_sync = state.sync().in_sync.get_untracked();
                let saved = state.save();
                let sent: Vec<String> = heard
                    .changes
                    .get_untracked()
                    .into_iter()
                    .chain(heard.saves.get_untracked())
                    .collect();
                let wrong: Vec<&String> = sent
                    .iter()
                    .filter(|text| {
                        super::super::codec::read(host.behavior(), text, None)
                            .ok()
                            .and_then(|read| read.pointer("/r/s").cloned())
                            != Some(value.clone())
                    })
                    .collect();
                if !wrong.is_empty() {
                    failures.push(format!(
                        "{case}: sent what reads back otherwise (in sync {in_sync}, saved {saved}): {wrong:?}"
                    ));
                    continue;
                }
                if sent.is_empty() {
                    let errors = state.sync().conversion_problems.with_untracked(|problems| {
                        problems
                            .iter()
                            .filter(|problem| {
                                problem.at.severity == super::super::diagnostics::Severity::Error
                            })
                            .count()
                    });
                    if in_sync || saved || errors == 0 {
                        failures.push(format!(
                            "{case}: nothing sent, yet in sync {in_sync}, saved {saved}, {errors} errors"
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}

/// A formatting-only edit (a space after the last line, a space before a
/// value, a blank line) converts back to the host's own text: once the
/// pause or a save converts it, the document is clean, and a save sends
/// the host's text unchanged.
#[test]
fn a_formatting_only_edit_is_clean_once_converted() {
    let mut failures = Vec::new();
    type Change = fn(&str) -> String;
    let edits: [(&str, Change); 3] = [
        ("a space after the last line", |text| format!("{text} ")),
        ("a blank line at the end", |text| format!("{text}\n")),
        ("a space at the start", |text| format!(" {text}")),
    ];
    for host in ALL {
        let host_text = host
            .serialize(&serde_json::json!({"listen": 7835, "name": "a"}))
            .unwrap();
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            for (label, change) in &edits {
                for save in [false, true] {
                    let case = format!("{host} host as {edit}, {label}, saved {save}");
                    let (_owner, state, heard) = mount(host, edit, &host_text);
                    let shown = state.doc().text.get_untracked();
                    let typed = change(&shown);
                    if edit.parse(&typed).is_err() {
                        continue;
                    }
                    replace_all(state, &typed);
                    if save {
                        if !state.save() {
                            failures.push(format!("{case}: not saved"));
                            continue;
                        }
                        if heard.saves.get_untracked() != vec![host_text.clone()] {
                            failures
                                .push(format!("{case}: saved {:?}", heard.saves.get_untracked()));
                        }
                    } else {
                        pause(state);
                    }
                    if state.editing().dirty.get_untracked() {
                        failures.push(format!("{case}: still dirty"));
                    }
                    // A real edit after it is dirty again.
                    let typed = state.doc().text.get_untracked().replace("7835", "7836");
                    replace_all(state, &typed);
                    pause(state);
                    if !state.editing().dirty.get_untracked() {
                        failures.push(format!("{case}: a value edit after it is clean"));
                    }
                }
            }
        }
    }
    report(failures);
}
