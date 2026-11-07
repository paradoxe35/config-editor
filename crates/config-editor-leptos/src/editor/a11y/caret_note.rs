//! What is at the caret, for screen readers: the textarea's
//! `aria-describedby` points at a hidden element holding the messages of
//! the problems on the caret's position (the editor's and the host's), and
//! the reason a read-only field is read-only or a secret placeholder's
//! hint. It follows the caret a moment after it stops, so holding an arrow
//! key does not rewrite it on every step.

use leptos::prelude::*;

use super::super::diagnostics::Diagnostic;
use super::super::keyboard::utf16_to_byte_index;
use super::super::state::EditorState;
use super::texts::problem_text;

/// How long the caret rests before the note follows it, in milliseconds.
#[cfg(target_arch = "wasm32")]
const NOTE_DELAY_MS: i32 = 150;

/// The one-based line and character column of UTF-16 `caret` in `text`.
pub(super) fn caret_position(text: &str, caret: usize) -> (usize, usize) {
    let byte = utf16_to_byte_index(text, caret);
    let line_start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
    let line = text[..line_start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    (line, text[line_start..byte].chars().count() + 1)
}

/// Whether `problem` covers the caret at one-based `line`:`column`: its
/// underline from the first character to just past the last (the caret
/// after a word is on it), or its whole line when it has no column.
pub(super) fn covers(problem: &Diagnostic, line: usize, column: usize) -> bool {
    if problem.line != Some(line) {
        return false;
    }
    let Some(start) = problem.column else {
        return true;
    };
    let end = problem.end_column.unwrap_or(start + 1).max(start + 1);
    start <= column && column <= end
}

/// The note for the caret at one-based `line`:`column`: each problem on
/// it, then what the protected text there is.
pub(super) fn note(
    problems: &[Diagnostic],
    line: usize,
    column: usize,
    protected: Option<(&str, String)>,
) -> String {
    problems
        .iter()
        .filter(|problem| covers(problem, line, column))
        .map(problem_text)
        .chain(protected.map(|(label, message)| format!("{label}: {message}")))
        .collect::<Vec<_>>()
        .join(". ")
}

/// Keeps the note in step with the caret, the problems and the locks.
pub(in crate::editor) fn connect_caret_note(state: EditorState) {
    Effect::new(move |_| {
        state.doc.caret.track();
        state.problems.diagnostics.track();
        state.protect.protected.track();
        state.protect.secrets.tokens.track();
        state.a11y.note_token.update_value(|token| *token += 1);
        let token = state.a11y.note_token.get_value();
        later(move || {
            if state.a11y.note_token.try_get_value() == Some(token) {
                update_note(state);
            }
        });
    });
}

/// Writes the note for where the caret is now.
pub(super) fn update_note(state: EditorState) {
    let Some(caret) = state.doc.caret.try_get_untracked() else {
        return;
    };
    let (line, column) = state
        .doc
        .text
        .with_untracked(|text| caret_position(text, caret));
    let protected = state.protected_note(caret, true);
    let note = state
        .problems
        .diagnostics
        .with_untracked(|problems| note(problems, line, column, protected));
    if state.a11y.caret_note.with_untracked(|shown| *shown != note) {
        state.a11y.caret_note.set(note);
    }
}

/// Runs `run` once the caret has rested; native unit tests have no window
/// to time it with, so there it runs now.
fn later(run: impl FnOnce() + 'static) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        use wasm_bindgen::JsCast;
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            wasm_bindgen::closure::Closure::once_into_js(run).unchecked_ref(),
            NOTE_DELAY_MS,
        );
        return;
    }
    run();
}

/// Shows what is at the caret as the pointer would: the message of the
/// problem there, or what the protected text there is. A tap on a phone
/// has no hover to show them with.
pub(in crate::editor) fn show_at_caret(state: EditorState) {
    let caret = state.doc.caret.get_untracked();
    let (line, column) = state
        .doc
        .text
        .with_untracked(|text| caret_position(text, caret));
    let problem = state.problems.diagnostics.with_untracked(|problems| {
        problems
            .iter()
            .find(|problem| covers(problem, line, column))
            .and_then(|problem| Some((problem.line?, problem.column?)))
    });
    match problem {
        Some(position) => state.problems.current.set(Some(position)),
        None => {
            state.hover_protected(line - 1, column - 1);
        }
    }
}
