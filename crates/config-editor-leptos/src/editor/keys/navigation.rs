//! Caret movement and history: smart Home, placeholders, undo/redo, F8 and
//! Escape.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::a11y::problem_announcement;
use super::super::commands::{cycle_offset, offset_at, smart_home as home_target};
use super::super::completion::visual_column;
use super::super::keyboard::utf16_to_byte_index;
use super::super::metrics::read_editor_metrics;
use super::super::positioning::reveal_offset;
use super::super::state::{EditorState, directed_selection, textarea_selection};

/// Smart Home: the first non-blank character, then the line start.
pub(super) fn smart_home(state: EditorState, event: &KeyboardEvent) -> bool {
    if event.key() != "Home" || event.ctrl_key() || event.meta_key() || event.alt_key() {
        return false;
    }
    let Some(element) = state.textarea() else {
        return false;
    };
    let (anchor, caret) = directed_selection(&element);
    let target = state
        .doc
        .text
        .with_untracked(|text| home_target(text, caret));
    event.prevent_default();
    if event.shift_key() {
        let direction = if target < anchor {
            "backward"
        } else {
            "forward"
        };
        let _ = element.set_selection_range_with_direction(
            anchor.min(target) as u32,
            anchor.max(target) as u32,
            direction,
        );
    } else {
        let _ = element.set_selection_range(target as u32, target as u32);
    }
    // Bring the caret back into view when the line is scrolled sideways.
    let column = state.doc.text.with_untracked(|text| {
        let byte = utf16_to_byte_index(text, target);
        let line_start = text[..byte].rfind('\n').map_or(0, |index| index + 1);
        visual_column(&text[line_start..byte])
    });
    let metrics = read_editor_metrics(element.element());
    let caret_x = column * metrics.char_width;
    if caret_x < f64::from(element.scroll_left()) {
        element.set_scroll_left((caret_x - 2.0 * metrics.char_width).max(0.0) as i32);
    }
    // And vertically, when the caret's line is scrolled out of view.
    state
        .doc
        .text
        .with_untracked(|text| reveal_offset(&element, text, target, state.view.scroll_y));
    state.note_caret(target);
    true
}

/// A default left selected after an accept is kept by Enter and Tab: Tab
/// steps past it (and its closing quote) and is done; Enter goes on from
/// the end of its line, so a string or element is never split.
pub(super) fn keep_placeholder(state: EditorState, event: &KeyboardEvent) -> bool {
    let Some((start, end)) = state.completion.placeholder.get_value() else {
        return false;
    };
    let key = event.key();
    if !matches!(key.as_str(), "Enter" | "Tab") || event.shift_key() {
        return false;
    }
    let Some(element) = state.textarea() else {
        return false;
    };
    state.completion.placeholder.set_value(None);
    if textarea_selection(&element) != (start, end) {
        return false;
    }
    let end = state.doc.text.with_untracked(|text| {
        let byte = utf16_to_byte_index(text, end);
        let rest = &text[byte..];
        let skip = if key == "Enter" {
            rest.find('\n').unwrap_or(rest.len())
        } else {
            usize::from(rest.starts_with(['"', '\'']))
        };
        end + rest[..skip].encode_utf16().count()
    });
    let _ = element.set_selection_range(end as u32, end as u32);
    state.doc.caret.set(end);
    state.close_suggestions();
    if key == "Tab" {
        event.prevent_default();
        // On to the block's next value, if it has one.
        state
            .doc
            .text
            .with_untracked(|text| super::tab::next_placeholder_slot(state, text, end));
        return true;
    }
    false
}

/// Ctrl+Z, and Ctrl+Shift+Z or Ctrl+Y.
pub(super) fn undo_redo(state: EditorState, event: &KeyboardEvent) -> bool {
    let key = event.key().to_ascii_lowercase();
    let modified = event.ctrl_key() || event.meta_key();
    let undo = modified && key == "z" && !event.shift_key();
    let redo = modified && ((key == "z" && event.shift_key()) || (event.ctrl_key() && key == "y"));
    if !undo && !redo {
        return false;
    }
    event.prevent_default();
    state.typing.closers.update_value(Vec::clear);
    state.completion.slot_region.set_value(None);
    // Undo never changes a read-only field the host locked meanwhile: the
    // pipeline refuses it, and the step stays.
    if let Some(edit) = state.history_edit(redo)
        && state.commit(edit)
    {
        state.close_suggestions();
    }
    true
}

/// F8 and Shift+F8 walk the problems by position, not by the order they
/// were found.
pub(super) fn next_problem(state: EditorState, event: &KeyboardEvent) -> bool {
    if event.code() != "F8" {
        return false;
    }
    event.prevent_default();
    // Right after an edit the problems are those of the text before the
    // typing pause: check the text as it is now first.
    let text = state.doc.text.get_untracked();
    if state
        .problems
        .diagnosed_text
        .with_untracked(|diagnosed| *diagnosed != text)
    {
        state.problems.diagnosed_text.set(text);
    }
    let here = state.doc.caret.get_untracked();
    let mut positioned: Vec<(usize, usize)> = state.doc.text.with_untracked(|text| {
        state.problems.diagnostics.with_untracked(|problems| {
            problems
                .iter()
                .enumerate()
                .filter_map(|(index, problem)| {
                    Some((offset_at(text, problem.line?, problem.column?), index))
                })
                .collect()
        })
    });
    positioned.sort_unstable();
    let offsets: Vec<usize> = positioned.iter().map(|(offset, _)| *offset).collect();
    if let Some(position) = cycle_offset(&offsets, here, event.shift_key()) {
        let index = positioned[position].1;
        state.go_to_diagnostic(index);
        // "Error 2 of 5: …" for screen readers, which do not see the
        // message the marker shows.
        let said = state.problems.diagnostics.with_untracked(|problems| {
            problems
                .get(index)
                .map(|problem| problem_announcement(problem, position + 1, positioned.len()))
        });
        if let Some(said) = said {
            state.a11y.announce(said);
        }
    }
    true
}

/// Escape closes the suggestion list, then the find widget, or with nothing
/// to close collapses a selection.
pub(super) fn escape(state: EditorState, event: &KeyboardEvent) {
    if state.completion.requested.get_untracked() {
        event.prevent_default();
        state.close_suggestions();
        return;
    }
    if super::super::search::close_on_escape(state) {
        event.prevent_default();
        return;
    }
    if let Some(element) = state.textarea() {
        let (start, end) = textarea_selection(&element);
        if start != end {
            let (_, caret) = directed_selection(&element);
            let _ = element.set_selection_range(caret as u32, caret as u32);
        }
    }
}
