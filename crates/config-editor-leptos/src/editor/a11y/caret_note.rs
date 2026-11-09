//! What is at the caret, for screen readers: the textarea's
//! `aria-describedby` points at a hidden element holding the messages of
//! the problems on the caret's position (the editor's and the host's), and
//! the reason a read-only field is read-only or a secret placeholder's
//! hint. It follows the caret a moment after it stops, so holding an arrow
//! key does not rewrite it on every step.

use leptos::prelude::*;

use super::super::diagnostics::Diagnostic;
use super::super::keyboard::utf16_to_byte_index;
use super::super::problem_hit::innermost_start;
use super::super::state::EditorState;
use super::texts::problem_text;
use crate::EditorStrings;
use crate::reactive::later;
use crate::strings::fill;

/// How long the caret rests before the note follows it, in milliseconds.
const NOTE_DELAY_MS: i32 = 150;

/// The one-based line and character column of UTF-16 `caret` in `text`.
pub(in crate::editor) fn caret_position(text: &str, caret: usize) -> (usize, usize) {
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
/// after a word is on it), over every line of a range, or its whole line
/// when it has no column.
pub(super) fn covers(problem: &Diagnostic, line: usize, column: usize) -> bool {
    let Some(first) = problem.line else {
        return false;
    };
    let last = problem.last_line().unwrap_or(first).max(first);
    if line < first || line > last {
        return false;
    }
    let Some(start) = problem.column else {
        return true;
    };
    if first == last {
        let end = problem.end_column.unwrap_or(start + 1).max(start + 1);
        return start <= column && column <= end;
    }
    (line > first || column >= start)
        && (line < last || problem.end_column.is_none_or(|end| column <= end))
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
        state.doc().caret.track();
        state.problems().diagnostics.track();
        state.protect().track();
        state.config().settings.strings.track();
        state.a11y().note_token.update_value(|token| *token += 1);
        let token = state.a11y().note_token.get_value();
        later::after(state, NOTE_DELAY_MS, move |state| {
            if state.a11y().note_token.get_value() == token {
                update_note(state);
            }
        });
    });
}

/// Writes the note for where the caret is now.
pub(super) fn update_note(state: EditorState) {
    let Some(caret) = state.doc().caret.try_get_untracked() else {
        return;
    };
    let (line, column) = state
        .doc()
        .text
        .with_untracked(|text| caret_position(text, caret));
    let protected = state.protected_note(caret, true);
    let note = state.strings_untracked(|strings| {
        state
            .problems()
            .diagnostics
            .with_untracked(|problems| note(strings, problems, line, column, protected))
    });
    if state
        .a11y()
        .caret_note
        .with_untracked(|shown| *shown != note)
    {
        state.a11y().caret_note.set(note);
    }
}

/// The docs link of the first problem at the caret that has one.
pub(in crate::editor) fn caret_problem_docs(state: EditorState) -> Option<String> {
    let caret = state.doc().caret.get_untracked();
    let (line, column) = state
        .doc()
        .text
        .with_untracked(|text| caret_position(text, caret));
    state.problems().diagnostics.with_untracked(|problems| {
        problems
            .iter()
            .filter(|problem| covers(problem, line, column))
            .find_map(|problem| problem.docs_url.clone())
    })
}

/// `Alt+Enter` opens a docs link in a new tab: a problem's at the caret,
/// else the shown hover card's (the field's Documentation link): the
/// keyboard's way to the links the pointer clicks.
pub(in crate::editor) fn open_docs_key(state: EditorState, event: &web_sys::KeyboardEvent) -> bool {
    let plain_alt = event.alt_key() && !event.ctrl_key() && !event.meta_key() && !event.shift_key();
    event.key() == "Enter" && plain_alt && open_docs(state, event)
}

/// Opens the docs link `Alt+Enter` opens (see [`open_docs_key`]), taking
/// `event`; false when there is none.
pub(in crate::editor) fn open_docs(state: EditorState, event: &web_sys::KeyboardEvent) -> bool {
    let card_docs = || {
        state.hover().card.with_untracked(|card| {
            card.as_ref()
                .filter(|_| state.completion().suggestions.with_untracked(Vec::is_empty))
                .and_then(|card| card.info.docs_url.clone())
        })
    };
    let Some(url) = caret_problem_docs(state).or_else(card_docs) else {
        return false;
    };
    event.prevent_default();
    if let Some(window) = web_sys::window() {
        let _ = window.open_with_url_and_target_and_features(&url, "_blank", "noopener");
    }
    true
}

/// The problems at the caret, as its note words them.
pub(in crate::editor) fn caret_problems_said(state: EditorState) -> String {
    let caret = state.doc().caret.get_untracked();
    let (line, column) = state
        .doc()
        .text
        .with_untracked(|text| caret_position(text, caret));
    state.strings_untracked(|strings| {
        state
            .problems()
            .diagnostics
            .with_untracked(|problems| note(strings, problems, line, column, None))
    })
}

/// Where the problems at the caret are, one-based: where the innermost
/// problem on the character after it (or, at a word's end, before it)
/// starts on its line (F8's place when it starts there, its underline
/// marked shown).
pub(in crate::editor) fn problems_at_caret(state: EditorState) -> Option<(usize, usize)> {
    let caret = state.doc().caret.get_untracked();
    state.doc().text.with_untracked(|text| {
        let (line, column) = caret_position(text, caret);
        let line_text = text.split('\n').nth(line - 1).unwrap_or("");
        let at = state.problems().diagnostics.with_untracked(|problems| {
            [Some(column), column.checked_sub(1).filter(|at| *at > 0)]
                .into_iter()
                .flatten()
                .find_map(|at| innermost_start(problems, line, at, line_text))
        });
        at.map(|at| (line, at))
    })
}

/// Shows what is at the caret as the pointer would: the messages of the
/// problems there and the key's docs (the hover card, listing those
/// problems when it shows), else what the protected text there is. A tap
/// on a phone has no hover to show them with; `Ctrl+K Ctrl+I` is the
/// keyboard's way.
pub(in crate::editor) fn show_at_caret(state: EditorState) {
    let place = problems_at_caret(state);
    if place.is_some() {
        state.problems().current.set(place);
        state.show_hover_at_caret();
        return;
    }
    let caret = state.doc().caret.get_untracked();
    let (line, column) = state
        .doc()
        .text
        .with_untracked(|text| caret_position(text, caret));
    if !state.hover_protected(line - 1, column - 1) {
        state.show_hover_at_caret();
    }
}
