//! Problem markers between validations: until the text is checked again,
//! each one follows the edits made since the check, so it never sits on
//! other text than the one it marks.

use std::sync::Arc;

use leptos::prelude::*;

use crate::editor::edit::{Change, Edit};
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::test_state::TestEditor;

const SCHEMA: &str = r#"{"type": "object", "additionalProperties": {"type": "integer"}}"#;

/// The text each marker covers in `text`.
fn marked(state: crate::editor::state::EditorState, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    state
        .problems()
        .diagnostics
        .get_untracked()
        .iter()
        .filter_map(|problem| {
            let line = lines.get(problem.line? - 1)?;
            let (start, end) = (problem.column? - 1, problem.end_column? - 1);
            Some(line.chars().skip(start).take(end - start).collect())
        })
        .collect()
}

/// An edit at two places on two lines (as Replace All makes it) moves each
/// marker after them on its line by what was written before it.
#[test]
fn markers_follow_an_edit_at_two_places_on_their_lines() {
    let text = "xx: one\nxx2: two\n";
    let mut editor = TestEditor::new(Language::YAML, text);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema")));
    let (_owner, state) = editor.mount();
    state.problems().diagnosed_text.set(text.to_string());
    assert_eq!(marked(state, text), ["one", "two"]);

    let change = Change::replacing(
        text.len(),
        [(0..2, "x".to_string()), (8..10, "x".to_string())],
    )
    .expect("fits");
    assert!(state.commit(Edit::command(text.to_string(), change, (0, 0))));
    let edited = state.doc().text.get_untracked();
    assert_eq!(edited, "x: one\nx2: two\n");
    assert_eq!(marked(state, &edited), ["one", "two"]);
}

/// A line added above a marker on a line like the one written moves the
/// marker down with its own line, not onto the added one.
#[test]
fn a_marker_stays_on_its_line_when_a_line_like_it_is_added_above() {
    let text = "a: 1\nb: bad\n";
    let mut editor = TestEditor::new(Language::YAML, text);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema")));
    let (_owner, state) = editor.mount();
    state.problems().diagnosed_text.set(text.to_string());
    let first = state.problems().diagnostics.get_untracked();
    assert_eq!(
        first.iter().map(|problem| problem.line).collect::<Vec<_>>(),
        [Some(2)]
    );

    // `b: bad\n` written at the start of line 2: the marked line is now 3.
    let change = Change::replacing(text.len(), [(5..5, "b: bad\n".to_string())]).expect("fits");
    assert!(state.commit(Edit::command(text.to_string(), change, (5, 5))));
    let lines: Vec<Option<usize>> = state
        .problems()
        .diagnostics
        .get_untracked()
        .iter()
        .map(|problem| problem.line)
        .collect();
    assert_eq!(lines, [Some(3)]);
}
