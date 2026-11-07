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
use crate::EditorStrings;
use crate::reactive::later;
use crate::strings::fill;

/// How long the caret rests before the note follows it, in milliseconds.
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
    strings: &EditorStrings,
    problems: &[Diagnostic],
    line: usize,
    column: usize,
    protected: Option<(String, String)>,
) -> String {
    problems
        .iter()
        .filter(|problem| covers(problem, line, column))
        .map(|problem| problem_text(strings, problem))
        .chain(protected.map(|(label, message)| {
            fill(
                &strings.protected_note,
                &[("label", &label), ("message", &message)],
            )
        }))
        .collect::<Vec<_>>()
        .join(". ")
}

/// Keeps the note in step with the caret, the problems and the locks.
pub(in crate::editor) fn connect_caret_note(state: EditorState) {
    Effect::new(move |_| {
        state.doc.caret.track();
        state.problems.diagnostics.track();
        state.protect.track();
        state.config.settings.strings.track();
        state.a11y.note_token.update_value(|token| *token += 1);
        let token = state.a11y.note_token.get_value();
        later::after(state, NOTE_DELAY_MS, move |state| {
            if state.a11y.note_token.get_value() == token {
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
    let note = state.strings_untracked(|strings| {
        state
            .problems
            .diagnostics
            .with_untracked(|problems| note(strings, problems, line, column, protected))
    });
    if state.a11y.caret_note.with_untracked(|shown| *shown != note) {
        state.a11y.caret_note.set(note);
    }
}

/// `Alt+Enter` on a problem with a docs link opens it in a new tab: the
/// keyboard's way to the link the pointer clicks under the message.
pub(in crate::editor) fn open_docs_key(state: EditorState, event: &web_sys::KeyboardEvent) -> bool {
    let plain_alt = event.alt_key() && !event.ctrl_key() && !event.meta_key() && !event.shift_key();
    if event.key() != "Enter" || !plain_alt {
        return false;
    }
    let caret = state.doc.caret.get_untracked();
    let (line, column) = state
        .doc
        .text
        .with_untracked(|text| caret_position(text, caret));
    let url = state.problems.diagnostics.with_untracked(|problems| {
        problems
            .iter()
            .filter(|problem| covers(problem, line, column))
            .find_map(|problem| problem.docs_url.clone())
    });
    let Some(url) = url else {
        return false;
    };
    event.prevent_default();
    if let Some(window) = web_sys::window() {
        let _ = window.open_with_url_and_target_and_features(&url, "_blank", "noopener");
    }
    true
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
