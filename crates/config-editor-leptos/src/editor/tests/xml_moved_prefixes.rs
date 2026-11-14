//! QA `E36-3`: an element with a prefix (or a prefixed attribute or
//! child) cut in an Edit-as view and pasted under another element, out of
//! the element declaring that prefix: no view shows the declaration or
//! the prefix, so the user could not declare it. The moved element
//! declares it itself, bound as the host bound it, when the paste
//! follows a pause or a switch to another view, from every view.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use crate::editor::save_matrix::report;
use leptos::prelude::*;

struct Case {
    name: &'static str,
    host: &'static str,
    /// The key moved, the sibling it is pasted before (in a TOML view a
    /// table is pasted at the end, under `table`).
    key: &'static str,
    sibling: &'static str,
    table: &'static str,
    /// What the save holds.
    expected: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "element with an attribute",
        host: "<root>\n  <p xmlns:c=\"urn:c\">\n    <c:scan pkg=\"x\"/>\n    <k>0</k>\n  </p>\n  <q>\n    <z>1</z>\n  </q>\n</root>\n",
        key: "scan",
        sibling: "z",
        table: "root.q",
        expected: "<c:scan xmlns:c=\"urn:c\" pkg=\"x\"/>",
    },
    Case {
        name: "prefixed attribute and child",
        host: "<root>\n  <p xmlns:d=\"urn:d\">\n    <s d:a=\"1\">\n      <d:t>2</d:t>\n    </s>\n    <k>0</k>\n  </p>\n  <q>\n    <z>1</z>\n  </q>\n</root>\n",
        key: "s",
        sibling: "z",
        table: "root.q",
        expected: "<s xmlns:d=\"urn:d\" d:a=\"1\">\n      <d:t>2</d:t>\n    </s>",
    },
    Case {
        name: "SOAP element out of the declaring element",
        host: "<soap:Envelope xmlns:soap=\"urn:s\">\n  <soap:Body>\n    <m:Get xmlns:m=\"urn:m\">\n      <m:Name>T</m:Name>\n      <m:Cur>USD</m:Cur>\n    </m:Get>\n    <soap:Id>7</soap:Id>\n  </soap:Body>\n</soap:Envelope>\n",
        key: "Name",
        sibling: "Id",
        table: "Envelope.Body",
        expected: "<m:Name xmlns:m=\"urn:m\">T</m:Name>",
    },
];

fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn line_offset(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line).map(str::len).sum()
}

/// Whether `line` writes the key `name` (`name:`, `"name":`, `name =`).
fn writes_key(line: &str, name: &str) -> bool {
    let line = line.trim_start().trim_start_matches('"');
    line.strip_prefix(name)
        .is_some_and(|rest| rest.starts_with(['"', ':', ' ']))
}

/// The lines (first, past the last) of the entry `name` in `view`'s text:
/// a key's line, the block it opens, or a TOML table.
fn entry_lines(view: Language, text: &str, name: &str) -> (usize, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines
        .iter()
        .position(|line| writes_key(line, name) || line.ends_with(&format!(".{name}]")))
        .unwrap_or_else(|| panic!("no {name} in {text}"));
    let opens = lines[first].trim_end().ends_with(['{', ':']);
    let last = if lines[first].starts_with('[') {
        (first + 1..lines.len())
            .find(|at| lines[*at].starts_with('['))
            .unwrap_or(lines.len())
    } else if !opens {
        first + 1
    } else if view == Language::YAML {
        (first + 1..lines.len())
            .find(|at| indent(lines[*at]) <= indent(lines[first]))
            .unwrap_or(lines.len())
    } else {
        (first + 1..lines.len())
            .find(|at| indent(lines[*at]) == indent(lines[first]))
            .map_or(lines.len(), |at| at + 1)
    };
    (first, last)
}

/// The entry `key` cut from the view's text, as a user cuts its lines.
fn cut(state: EditorState, view: Language, key: &str) -> String {
    let before = state.doc().text.get_untracked();
    let (start, end) = entry_lines(view, &before, key);
    let (from, to) = (line_offset(&before, start), line_offset(&before, end));
    let lines = before[from..to].to_string();
    let text = format!("{}{}", &before[..from], &before[to..]);
    let caret = units(&before[..from]);
    state.commit(Edit::new(before, (caret, caret), text, caret));
    lines
}

/// `lines` pasted before the entry `sibling`, indented as it; a TOML
/// table (`[a.p.scan]`) is pasted at the end, as a table of `table`.
fn paste(state: EditorState, lines: &str, sibling: &str, table: &str) {
    let before = state.doc().text.get_untracked();
    let (at, inserted) = match lines.strip_prefix('[') {
        Some(header) => {
            let (name, rest) = header.split_once(']').expect("a table header");
            let key = name.rsplit('.').next().unwrap_or(name);
            (before.len(), format!("[{table}.{key}]{rest}"))
        }
        None => {
            let shown: Vec<&str> = before.lines().collect();
            let target = shown
                .iter()
                .position(|line| writes_key(line, sibling))
                .unwrap_or_else(|| panic!("no {sibling} in {before}"));
            let (to, from) = (indent(shown[target]), indent(lines));
            let moved: String = lines
                .split_inclusive('\n')
                .map(|line| match to >= from {
                    true => format!("{}{line}", " ".repeat(to - from)),
                    false => line[from - to..].to_string(),
                })
                .collect();
            (line_offset(&before, target), moved)
        }
    };
    let text = format!("{}{inserted}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    let caret = start + units(&inserted);
    state.commit(Edit::new(before, (start, start), text, caret));
}

#[test]
fn an_element_moved_out_of_its_prefix_scope_declares_the_prefix() {
    let mut failures = Vec::new();
    for case in CASES {
        for view in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            for switched in [false, true] {
                let mut editor = TestEditor::new(Language::XML, case.host);
                editor.edit_as = Some(view);
                let (_owner, state) = editor.mount();
                let lines = cut(state, view, case.key);
                pause(state);
                if switched {
                    let other = if view == Language::YAML {
                        Language::JSON
                    } else {
                        Language::YAML
                    };
                    switch_to(state, other);
                    switch_to(state, view);
                }
                paste(state, &lines, case.sibling, case.table);
                pause(state);
                let saved = state.sync().host_text.get_value();
                let problems = crate::check_syntax(Language::XML, &saved);
                if !saved.contains(case.expected) || !problems.is_empty() {
                    let waits: Vec<String> = state
                        .sync()
                        .conversion_problems
                        .get_untracked()
                        .iter()
                        .map(|problem| format!("{:?}", problem.text))
                        .collect();
                    failures.push(format!(
                        "{} as {view:?}, switched {switched}: {waits:?}\n{}\n{saved}",
                        case.name,
                        state.doc().text.get_untracked()
                    ));
                }
            }
        }
    }
    report(failures);
}
