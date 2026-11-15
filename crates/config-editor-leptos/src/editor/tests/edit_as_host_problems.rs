//! An Edit-as view shows the host's schema problems its own reading
//! cannot see: a JSON host's `"8"` under `type: number` reads as `8` in an
//! XML view, which keeps the host's text, and its problem, while the value
//! is untouched. A problem both readings see shows once.

use std::sync::Arc;

use leptos::prelude::*;

use super::super::diagnostics::{Diagnostic, Severity, build_schema};
use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;

const SCHEMA: &str = r#"{"type": "object", "properties": {
    "port": {"type": "number"}, "size": {"type": "number"},
    "name": {"type": "string"}}}"#;

fn mounted(host: &str) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::JSON, host);
    editor.edit_as = Some(Language::XML);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).unwrap()));
    let mounted = editor.mount();
    let shown = mounted.1.doc().text.get_untracked();
    mounted.1.problems().diagnosed_text.set(shown);
    mounted
}

/// The errors shown about the value at `pointer`.
fn errors_at(state: EditorState, pointer: &str) -> Vec<Diagnostic> {
    let prefix = format!("{pointer}: ");
    state
        .problems()
        .own
        .get_untracked()
        .into_iter()
        .filter(|problem| {
            problem.severity == Severity::Error && problem.message.starts_with(&prefix)
        })
        .collect()
}

/// The view's text with `from` typed over as `to`, after the typing pause.
fn typed(state: EditorState, from: &str, to: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.find(from).expect("the text");
    let text = before.replacen(from, to, 1);
    let caret = before[..at].encode_utf16().count() + to.encode_utf16().count();
    state.commit(Edit::new(before, (caret, caret), text.clone(), caret));
    state.problems().diagnosed_text.set(text.clone());
    convert_now(state, text, Language::XML);
}

#[test]
fn an_untouched_host_value_shows_its_host_problem_in_the_view() {
    let (_owner, state) = mounted("{\n  \"port\": \"8\",\n  \"name\": \"n\"\n}\n");
    let shown = state.doc().text.get_untracked();
    let port = errors_at(state, "/port");
    assert_eq!(port.len(), 1, "{shown}{port:?}");
    let line = shown
        .lines()
        .position(|line| line.contains("<port>"))
        .unwrap()
        + 1;
    assert_eq!(port[0].line, Some(line), "{shown}{port:?}");
    // Another value typed: the host keeps the text, and the problem.
    typed(state, "<name>n</name>", "<name>m</name>");
    assert!(state.sync().host_text.get_value().contains("\"8\""));
    assert_eq!(errors_at(state, "/port").len(), 1);
    // The value typed again: the host holds the number, which is right.
    typed(state, "<port>8</port>", "<port>9</port>");
    assert!(state.sync().host_text.get_value().contains("9"));
    assert_eq!(errors_at(state, "/port"), Vec::new());
}

#[test]
fn a_problem_both_readings_see_shows_once() {
    let (_owner, state) = mounted("{\n  \"port\": \"8\",\n  \"size\": \"x\"\n}\n");
    assert_eq!(errors_at(state, "/port").len(), 1);
    let size = errors_at(state, "/size");
    assert_eq!(size.len(), 1, "{size:?}");
}

/// One check of the host's text each time it changes, and none while
/// the view shows the host exactly (P5).
#[test]
fn the_host_is_checked_once_per_change_and_only_when_the_view_is_inexact() {
    use crate::editor::work_count::{Work, counted};
    let (_owner, state) = mounted("{\n  \"port\": 8,\n  \"size\": \"x\"\n}\n");
    let (size, checks) = counted(Work::HostChecked, || errors_at(state, "/size"));
    assert_eq!((size.len(), checks), (1, 0), "{size:?}");
    let (_owner, state) = mounted("{\n  \"port\": \"8\",\n  \"name\": \"n\"\n}\n");
    let (_, checks) = counted(Work::HostChecked, || {
        errors_at(state, "/port");
        typed(state, "<name>n</name>", "<name>m</name>");
        errors_at(state, "/port");
        // Read again with nothing changed: no check.
        errors_at(state, "/port")
    });
    assert_eq!(checks, 2);
}
