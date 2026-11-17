//! A YAML anchor key moved in an Edit-as view: moved
//! above another key, still before its aliases, they stay aliases, at
//! once or cut and pasted with a typing pause between (the cut's save
//! writes them out, the paste brings the anchor back); moved below its
//! aliases, the save waits naming the key moved, not another.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{line_offset, pause, units};
use crate::editor::save_matrix::report;
use leptos::prelude::*;

const HOST: &str = "x-env: &env\n  A: \"1\"\nport: &p 80\nsvc:\n  web:\n    environment: *env\n    port: *p\n  api:\n    environment: *env\n";

const VIEWS: [Language; 4] = [Language::JSON, Language::TOML, Language::HCL, Language::XML];

/// The lines of the top-level key `name` cut: its line, those below it
/// indented deeper, and the line closing what it opens (or, for a TOML
/// table, its lines to the next blank one).
fn cut(state: EditorState, name: &str) -> String {
    let before = state.doc().text.get_untracked();
    let lines: Vec<&str> = before.split_inclusive('\n').collect();
    let first = top_line(&before, name);
    let indent = |line: &str| line.len() - line.trim_start().len();
    let mut end = first + 1;
    if lines[first].starts_with('[') {
        while end < lines.len() && !lines[end].trim().is_empty() {
            end += 1;
        }
    } else {
        while end < lines.len() && indent(lines[end]) > indent(lines[first]) {
            end += 1;
        }
        let code = lines[first].trim();
        let opens = code.ends_with(['{', '[']) || (code.starts_with('<') && !code.contains("</"));
        if opens {
            end += 1;
        }
    }
    let (from, to) = (line_offset(&before, first), line_offset(&before, end));
    let cut = before[from..to].to_string();
    let text = format!("{}{}", &before[..from], &before[to..]);
    let at = units(&before[..from]);
    state.commit(Edit::new_units(before, (at, at + units(&cut)), text, at));
    cut
}

/// `lines` pasted at byte `at` of the view's text.
fn paste_at(state: EditorState, lines: &str, at: usize) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{lines}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    state.commit(Edit::new_units(
        before,
        (start, start),
        text,
        start + units(lines),
    ));
}

/// The view's text with a JSON view's commas made right.
fn commas(state: EditorState, view: Language) {
    if view != Language::JSON {
        return;
    }
    let before = state.doc().text.get_untracked();
    let mut lines: Vec<String> = before.lines().map(str::to_string).collect();
    for at in 0..lines.len() {
        let next = lines[at + 1..]
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty());
        let Some(next) = next else { continue };
        let code = lines[at].trim_end().to_string();
        if code.ends_with(',') && next.starts_with(['}', ']']) {
            lines[at] = code[..code.len() - 1].to_string();
        } else if !code.ends_with([',', '{', '[']) && next.starts_with('"') {
            lines[at] = format!("{code},");
        }
    }
    let text = format!("{}\n", lines.join("\n"));
    if text != before {
        let end = units(&before);
        state.commit(Edit::new_units(before, (0, end), text, 0));
    }
}

/// Whether the view writes the top-level keys inside one line pair (a
/// JSON object's braces).
fn wrapped(view: Language) -> bool {
    view == Language::JSON
}

fn mounted(view: Language) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::YAML, HOST);
    editor.edit_as = Some(view);
    editor.mount()
}

/// The first line of the view's text holding the top-level keys (past a
/// JSON view's `{`, an XML view's root element).
fn top(state: EditorState) -> usize {
    let text = state.doc().text.get_untracked();
    line_offset(&text, top_line(&text, "x-env"))
}

/// The line (from 0) writing the top-level key `name`, in any view.
fn top_line(text: &str, name: &str) -> usize {
    let forms = [
        format!("\"{name}\""),
        format!("<{name}>"),
        format!("[{name}]"),
        format!("{name} "),
    ];
    text.lines()
        .position(|line| {
            forms
                .iter()
                .any(|form| line.trim_start().starts_with(form.as_str()))
        })
        .unwrap_or_else(|| panic!("no {name} in\n{text}"))
}

#[test]
fn an_anchor_moved_still_before_its_aliases_keeps_them() {
    let expected = "port: &p 80\nx-env: &env\n  A: \"1\"\nsvc:\n  web:\n    environment: *env\n    port: *p\n  api:\n    environment: *env\n";
    let mut failures = Vec::new();
    // TOML writes `port` first already.
    for view in VIEWS.into_iter().filter(|view| *view != Language::TOML) {
        for with_pause in [false, true] {
            let (_owner, state) = mounted(view);
            let lines = cut(state, "port");
            if with_pause {
                commas(state, view);
                pause(state);
            }
            paste_at(state, &lines, top(state));
            commas(state, view);
            pause(state);
            let saved = state.sync().host_text.get_value();
            if saved != expected || !state.sync().in_sync.get_untracked() {
                failures.push(format!(
                    "via {view}, pause {with_pause}: {:?}\n{saved}",
                    state.sync().conversion_problems.get_untracked()
                ));
            }
        }
    }
    report(failures);
}

#[test]
fn an_anchor_moved_below_its_aliases_waits_naming_it() {
    let mut failures = Vec::new();
    for view in VIEWS {
        let (_owner, state) = mounted(view);
        let lines = cut(state, "x-env");
        let text = state.doc().text.get_untracked();
        // At the end of the top-level keys.
        let end = if wrapped(view) {
            text.trim_end().rfind('\n').map_or(0, |at| at + 1)
        } else {
            text.len()
        };
        let lines = if view == Language::TOML {
            format!("\n{lines}")
        } else {
            lines
        };
        paste_at(state, &lines, end);
        commas(state, view);
        pause(state);
        let problems = format!("{:?}", state.sync().conversion_problems.get_untracked());
        if state.sync().in_sync.get_untracked() || !problems.contains("(at /x-env)") {
            failures.push(format!(
                "via {view}: {problems}\n{}\nview:\n{}",
                state.sync().host_text.get_value(),
                state.doc().text.get_untracked()
            ));
        }
    }
    report(failures);
}
