//! The work one keystroke costs on a large document (the demo's 2,000-line
//! samples), counted (see `work_count`): what the view and the suggestions
//! read of the whole text after an edit is read once, however many of them
//! ask for it.

use leptos::prelude::*;

use super::diagnostics::SchemaBundle;
use super::edit::Edit;
use super::languages::Language;
use super::state::EditorState;
use super::test_state::TestEditor;
use super::work_count::{Work, counted};
use crate::reactive::later::testing::{hold, run_held};

/// The samples, each with a value in the middle to type into.
fn samples() -> Vec<(Language, &'static str)> {
    vec![
        (
            Language::YAML,
            include_str!("../../tests/fixtures/samples/tunnel.large.yaml"),
        ),
        (
            Language::JSON,
            include_str!("../../tests/fixtures/samples/tunnel.large.json"),
        ),
        (
            Language::TOML,
            include_str!("../../tests/fixtures/samples/tunnel.large.toml"),
        ),
        (
            Language::HCL,
            include_str!("../../tests/fixtures/samples/tunnel.large.hcl"),
        ),
        (
            Language::XML,
            include_str!("../../tests/fixtures/samples/tunnel.large.xml"),
        ),
    ]
}

fn schema() -> std::sync::Arc<SchemaBundle> {
    std::sync::Arc::new(
        super::diagnostics::build_schema(include_str!("../../tests/fixtures/tunnel.schema.json"))
            .unwrap(),
    )
}

/// Everything the mounted view reads after an edit: the lines, the code
/// layer's multi-line constructs, the fold markers, the suggestions and
/// the change bars.
fn read_view(state: EditorState) {
    let _ = state.view().lines.get_untracked();
    let _ = state.view().visible_lines.get_untracked();
    let _ = state.view().spanning.get_untracked();
    let _ = state.view().string_bodies.get_untracked();
    let _ = state.fold().fold_ranges.get_untracked();
    let _ = state.completion().suggestions.get_untracked();
    state.changes().diff.with_untracked(|_| ());
}

/// The UTF-16 offset of the end of the first line past the middle of
/// `text` that ends in a letter or digit (inside a value).
fn middle_value_end(text: &str) -> usize {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut offset: usize = lines[..lines.len() / 2]
        .iter()
        .map(|line| line.encode_utf16().count() + 1)
        .sum();
    for line in &lines[lines.len() / 2..] {
        let value = line.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
        let comment = value.trim_start().starts_with(['#', '/']) || value.contains("<!--");
        if value.contains([':', '=', '>']) && !comment {
            return offset + value.encode_utf16().count();
        }
        offset += line.encode_utf16().count() + 1;
    }
    panic!("no value to type into");
}

/// Types `x` at UTF-16 `at`, as the browser's input does.
fn type_x(state: EditorState, at: usize) {
    let before = state.doc().text.get_untracked();
    let byte = super::keyboard::utf16_to_byte_index(&before, at);
    let mut text = before.clone();
    text.insert(byte, 'x');
    state.completion().requested.set(true);
    let typed = Edit::input(
        before,
        (at, at),
        text,
        (at + 1, at + 1),
        "insertText".into(),
        0.0,
    );
    assert!(state.commit(typed));
}

#[test]
fn a_keystroke_reads_the_string_bodies_and_the_caret_s_place_once() {
    let schema = schema();
    // As a mounted editor does (`sync::connect`).
    super::parse_cache::hold();
    for (language, text) in samples() {
        let (_owner, state) = TestEditor {
            schema: Some(schema.clone()),
            ..TestEditor::new(language, text)
        }
        .mount();
        read_view(state);
        let at = middle_value_end(text);
        let ((((), reads), places), lexical) = counted(Work::LexicalState, || {
            counted(Work::CompletionContext, || {
                counted(Work::StringBodies, || {
                    type_x(state, at);
                    read_view(state);
                })
            })
        });
        assert_eq!(reads, 1, "{}: string bodies", language.label());
        assert_eq!(places, 1, "{}: the caret's place", language.label());
        assert_eq!(lexical, 1, "{}: in a string or a comment", language.label());
    }
    super::parse_cache::release();
}

/// The suggestions typing in a value offers (its values, and the children
/// a word there may name, less those already written) read the text once,
/// for the caret's place: the keys already written are read around the
/// caret's element, not off every span of the document (a second read of
/// the whole text, most of an XML keystroke's suggestions).
#[test]
fn a_keystroke_in_a_value_reads_the_whole_text_once_for_its_suggestions() {
    let schema = schema();
    super::parse_cache::hold();
    for (language, text) in samples() {
        let (_owner, state) = TestEditor {
            schema: Some(schema.clone()),
            ..TestEditor::new(language, text)
        }
        .mount();
        read_view(state);
        let at = middle_value_end(text);
        let (((), places), scans) = counted(Work::SpanScan, || {
            counted(Work::CompletionContext, || {
                type_x(state, at);
                let _ = state.completion().suggestions.get_untracked();
            })
        });
        let label = language.label();
        assert_eq!(
            (places, scans),
            (1, 0),
            "{label}: (the caret's place, every span)"
        );
    }
    super::parse_cache::release();
}

/// How many times `run` read the whole text for its string bodies, its
/// constructs over lines, its fold ranges and its changes.
fn readings(run: impl FnOnce()) -> [usize; 4] {
    let (((((), diffs), bodies), constructs), folds) = counted(Work::FoldRanges, || {
        counted(Work::Spanning, || {
            counted(Work::StringBodies, || counted(Work::LineDiff, run))
        })
    });
    [bodies, constructs, folds, diffs]
}

#[test]
fn a_keystroke_keeping_the_lines_reads_the_whole_text_once_the_frame_is_drawn() {
    super::parse_cache::hold();
    for (language, text) in samples() {
        hold(|| {
            let (_owner, state) = TestEditor::new(language, text).mount();
            read_view(state);
            let at = middle_value_end(text);
            let label = language.label();
            // The frame after the keystroke shows the readings as they were.
            assert_eq!(
                readings(|| {
                    type_x(state, at);
                    read_view(state);
                }),
                [0; 4],
                "{label}: before the frame"
            );
            // Once it is drawn, each is read once, for both keystrokes.
            type_x(state, at + 1);
            assert_eq!(
                readings(|| {
                    run_held();
                    run_held();
                    read_view(state);
                }),
                [1; 4],
                "{label}: after the frame"
            );
            assert!(state.view().string_bodies.with_untracked(|_| true));
            // Something acting on them reads them first.
            type_x(state, at + 2);
            assert_eq!(
                readings(|| {
                    let _ = state.in_string_body(at);
                    read_view(state);
                }),
                [1; 4],
                "{label}: read for a key"
            );
            // A new line moves the lines below: read at once.
            let before = state.doc().text.get_untracked();
            let byte = super::keyboard::utf16_to_byte_index(&before, at);
            let mut broken = before.clone();
            broken.insert(byte, '\n');
            assert_eq!(
                readings(|| {
                    assert!(state.commit(Edit::new(before, (at, at), broken, at + 1)));
                    read_view(state);
                }),
                [1; 4],
                "{label}: a line break"
            );
            // Closed folds stay only where the new text still folds.
            state.fold_all();
            let ((), folds) = counted(Work::FoldRanges, || type_x(state, 0));
            assert_eq!(folds, 1, "{label}: typing beside closed folds");
        });
    }
    super::parse_cache::release();
}
