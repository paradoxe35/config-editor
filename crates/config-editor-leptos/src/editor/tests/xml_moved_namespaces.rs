//! An element cut in an Edit-as view and pasted under another element
//! keeps its namespace: no view shows a declaration or a prefix, so the
//! user could neither see nor fix one that changed. Moved out
//! of the element declaring its prefix (or a prefixed attribute's or
//! child's), it declares the prefix itself, bound as the host bound it.
//! Moved into or out of a default namespace (`xmlns="…"`), or
//! where its prefix is bound to another namespace, it declares its own
//! (`xmlns=""`, `xmlns="urn:d"`, `xmlns:c="urn:c"`). Moved within its
//! namespace, it declares nothing. From every view, pasted at once, after
//! a pause or after a switch to another view, into another element or out
//! to an ancestor. A copy is a new element: it takes its siblings'
//! namespace.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use crate::editor::save_matrix::report;
use crate::editor::text::text_size;
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
    Case {
        name: "into a default namespace",
        host: "<root>\n  <p>\n    <t a=\"1\">x</t>\n    <k>0</k>\n  </p>\n  <q xmlns=\"urn:d\">\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "<t xmlns=\"\" a=\"1\">x</t>",
    },
    Case {
        name: "out of a default namespace",
        host: "<root>\n  <p xmlns=\"urn:d\">\n    <t a=\"1\">x</t>\n    <k>0</k>\n  </p>\n  <q>\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "<t xmlns=\"urn:d\" a=\"1\">x</t>",
    },
    Case {
        name: "out of a default namespace to its parent",
        host: "<root>\n  <p xmlns=\"urn:d\">\n    <t a=\"1\">x</t>\n    <k>0</k>\n  </p>\n  <z>1</z>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root",
        expected: "<t xmlns=\"urn:d\" a=\"1\">x</t>",
    },
    Case {
        name: "prefixed element out to its parent, where its prefix is bound otherwise",
        host: "<root xmlns:c=\"urn:other\">\n  <p xmlns:c=\"urn:c\">\n    <c:t a=\"1\">x</c:t>\n    <k>0</k>\n  </p>\n  <z>1</z>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root",
        expected: "<c:t xmlns:c=\"urn:c\" a=\"1\">x</c:t>",
    },
    Case {
        name: "from one default namespace to another, with a child",
        host: "<root xmlns=\"urn:a\">\n  <p>\n    <t a=\"1\">\n      <u>2</u>\n    </t>\n    <k>0</k>\n  </p>\n  <q xmlns=\"urn:b\">\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "<t xmlns=\"urn:a\" a=\"1\">\n      <u>2</u>\n    </t>",
    },
    Case {
        name: "prefix bound to another namespace where pasted",
        host: "<root>\n  <p xmlns:c=\"urn:c\">\n    <c:t a=\"1\">x</c:t>\n    <k>0</k>\n  </p>\n  <q xmlns:c=\"urn:other\">\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "<c:t xmlns:c=\"urn:c\" a=\"1\">x</c:t>",
    },
    Case {
        name: "prefixed attribute bound to another namespace where pasted",
        host: "<root>\n  <p xmlns:c=\"urn:c\">\n    <t c:a=\"1\">x</t>\n    <k>0</k>\n  </p>\n  <q xmlns:c=\"urn:other\">\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "<t xmlns:c=\"urn:c\" c:a=\"1\">x</t>",
    },
    Case {
        name: "prefixed element into a default namespace",
        host: "<root>\n  <p xmlns:c=\"urn:c\">\n    <c:t a=\"1\">x</c:t>\n    <k>0</k>\n  </p>\n  <q xmlns=\"urn:d\">\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "<c:t xmlns:c=\"urn:c\" a=\"1\">x</c:t>",
    },
    Case {
        name: "within its default namespace",
        host: "<root xmlns=\"urn:r\">\n  <p>\n    <t a=\"1\">x</t>\n    <k>0</k>\n  </p>\n  <q>\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "\n    <t a=\"1\">x</t>\n",
    },
    Case {
        name: "within its prefix's scope",
        host: "<root xmlns:c=\"urn:c\">\n  <p>\n    <c:t a=\"1\">x</c:t>\n    <k>0</k>\n  </p>\n  <q>\n    <z>1</z>\n  </q>\n</root>\n",
        key: "t",
        sibling: "z",
        table: "root.q",
        expected: "\n    <c:t a=\"1\">x</c:t>\n",
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
    let lines = copy(view, &before, key);
    let (start, end) = entry_lines(view, &before, key);
    let (from, to) = (line_offset(&before, start), line_offset(&before, end));
    let text = format!("{}{}", &before[..from], &before[to..]);
    let caret = units(&before[..from]);
    state.commit(Edit::new_units(before, (caret, caret), text, caret));
    lines
}

/// The entry `key`'s lines in the view's `text`, as a user copies them.
fn copy(view: Language, text: &str, key: &str) -> String {
    let (start, end) = entry_lines(view, text, key);
    text[line_offset(text, start)..line_offset(text, end)].to_string()
}

/// `lines` pasted before the entry `sibling`, indented as it; a TOML
/// table (`[a.p.scan]`) is pasted at the end, as a table of `table`.
fn paste(state: EditorState, lines: &str, sibling: &str, table: &str) {
    let before = state.doc().text.get_untracked();
    let (at, inserted) = match lines.strip_prefix('[') {
        Some(header) => {
            // Pasted at the end, its header then made to name `table`, as
            // typed.
            let (name, _) = header.split_once(']').expect("a table header");
            let parent = name.rsplit_once('.').map_or("", |(parent, _)| parent);
            let at = before.len();
            let text = format!("{before}{lines}");
            let start = units(&before);
            state.commit(Edit::new_units(
                before,
                (start, start),
                text.clone(),
                start + units(lines),
            ));
            let (from, to) = (at + 1, at + 1 + parent.len());
            let renamed = format!("{}{table}{}", &text[..from], &text[to..]);
            let (from, to) = (units(&text[..from]), units(&text[..to]));
            state.commit(Edit::new_units(
                text,
                (from, to),
                renamed,
                from + units(table),
            ));
            return;
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
    state.commit(Edit::new_units(before, (start, start), text, caret));
}

/// What happens between the cut and the paste.
#[derive(Clone, Copy, Debug)]
enum Between {
    Nothing,
    Pause,
    OtherView,
    /// Copied, not cut: the copy is pasted where the original stays.
    Copied,
}

#[test]
fn an_element_moved_keeps_its_namespace() {
    let mut failures = Vec::new();
    for case in CASES {
        for view in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            for step in [
                Between::Nothing,
                Between::Pause,
                Between::OtherView,
                Between::Copied,
            ] {
                let mut editor = TestEditor::new(Language::XML, case.host);
                editor.edit_as = Some(view);
                let (_owner, state) = editor.mount();
                let lines = match step {
                    Between::Copied => {
                        // As Ctrl+C copies them.
                        let text = state.doc().text.get_untracked();
                        let (start, end) = entry_lines(view, &text, case.key);
                        let (from, to) = (line_offset(&text, start), line_offset(&text, end));
                        state.note_copied(&text[from..to], (text_size(from), text_size(to)));
                        copy(view, &text, case.key)
                    }
                    _ => cut(state, view, case.key),
                };
                match step {
                    Between::Nothing | Between::Copied => {}
                    Between::Pause => pause(state),
                    Between::OtherView => {
                        pause(state);
                        let other = if view == Language::YAML {
                            Language::JSON
                        } else {
                            Language::YAML
                        };
                        switch_to(state, other);
                        switch_to(state, view);
                    }
                }
                paste(state, &lines, case.sibling, case.table);
                pause(state);
                let saved = state.sync().host_text.get_value();
                let problems = crate::check_syntax(Language::XML, &saved);
                // A copy keeps its source's namespace like a moved element
                // (a copy of an existing item keeps its form);
                // only a brand-new element takes its siblings'.
                let kept = saved != case.host && saved.contains(case.expected);
                if !kept || !problems.is_empty() {
                    let waits: Vec<String> = state
                        .sync()
                        .conversion_problems
                        .get_untracked()
                        .iter()
                        .map(|problem| format!("{:?}", problem.text))
                        .collect();
                    failures.push(format!(
                        "{} as {view:?}, {step:?}: {waits:?}\n{}\n{saved}",
                        case.name,
                        state.doc().text.get_untracked()
                    ));
                }
            }
        }
    }
    report(failures);
}
