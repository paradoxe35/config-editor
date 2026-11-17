//! Tab and Shift+Tab: placeholder slots, block indent, and tab stops.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{indent_lines_through, tab_insert};
use super::super::completion::next_stop;
use super::super::edit::{Change, Edit};
use super::super::state::{EditorState, is_backward, select, textarea_selection};
use crate::editor::text::{TextRange, TextSize, slice, utf16_of};

pub(super) fn tab(state: EditorState, event: &KeyboardEvent) {
    event.prevent_default();
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let text = state.doc().text.get_untracked();
    if !event.shift_key() && start == end {
        let in_block = in_slot_region(state, start);
        if next_placeholder_slot(state, &text, start) || step_out_of_value(state, &text, start) {
            return;
        }
        // Past the last value of a block a suggestion wrote: the block is
        // done, as at the end of a snippet, and Tab writes nothing.
        if in_block {
            state.close_suggestions();
            return;
        }
    }
    if event.shift_key() || spans_lines(&text, start, end) {
        indent_block(
            state,
            text,
            (start, end),
            event.shift_key(),
            is_backward(&element),
        );
        return;
    }
    // A selection within one line is replaced, as typing would.
    let insert = tab_insert(
        &text,
        utf16_of(&text, start),
        &state.doc().indent.get_value(),
    );
    let change = Change::replace(&text, TextRange::new(start, end.max(start)), &insert);
    state.commit(Edit::command(text, change, (start, end)));
    state.close_suggestions();
}

/// Whether `caret` is in the block a suggestion just wrote (whose values
/// Tab walks).
fn in_slot_region(state: EditorState, caret: TextSize) -> bool {
    state
        .completion()
        .slot_region
        .get_value()
        .is_some_and(|region| region.contains_inclusive(caret))
}

/// In a block a suggestion just wrote, Tab goes to its next empty value or
/// selects the next value it wrote, as through a snippet's placeholders.
pub(super) fn next_placeholder_slot(state: EditorState, text: &str, caret: TextSize) -> bool {
    let Some(region) = state.completion().slot_region.get_value() else {
        return false;
    };
    let next = region
        .contains_inclusive(caret)
        .then(|| {
            state
                .completion()
                .value_stops
                .with_value(|stops| next_stop(text, caret, region.end(), stops, state.behavior()))
        })
        .flatten();
    match next {
        Some((anchor, end)) => {
            state.place_caret(end);
            if anchor != end {
                // Selected, so typing replaces it; Tab again moves on.
                if let Some(element) = state.textarea() {
                    select(&element, anchor, end, false);
                }
                state
                    .completion()
                    .placeholder
                    .set_value(Some((anchor, end)));
            }
            true
        }
        None => {
            state.completion().slot_region.set_value(None);
            false
        }
    }
}

/// After a value typed before a closer the editor wrote (`"k.pem|"`,
/// `<listen>7836|</listen>`), Tab with no slot left steps past the closer,
/// as at the end of a snippet, instead of indenting inside the value.
fn step_out_of_value(state: EditorState, text: &str, caret: TextSize) -> bool {
    let Some(closer) = state.closer_at(caret) else {
        return false;
    };
    let Some(opener) = closer.opener else {
        return false;
    };
    if slice::before(text, caret).is_some_and(|head| head.ends_with(opener)) {
        return false;
    }
    state.forget_closer_at(caret);
    state.place_caret(caret + closer.len);
    true
}

/// Whether Tab indents the selected lines rather than replacing the
/// selection (bytes `start..end`): it spans lines, or covers one whole line
/// (as in VS Code).
fn spans_lines(text: &str, start: TextSize, end: TextSize) -> bool {
    let Some(selected) = slice::slice(text, TextRange::new(start, end.max(start))) else {
        return false;
    };
    if selected.is_empty() {
        return false;
    }
    if selected.contains('\n') {
        return true;
    }
    let Some(head) = slice::before(text, start) else {
        return false;
    };
    let line_start = start - TextSize::of(head.rsplit('\n').next().unwrap_or_default());
    let Some(line) = slice::after(text, line_start).and_then(|rest| rest.split('\n').next()) else {
        return false;
    };
    let indent = TextSize::of(line) - TextSize::of(line.trim_start_matches([' ', '\t']));
    let (indent_end, content_end) = (line_start + indent, line_start + TextSize::of(line));
    start <= indent_end && end >= content_end
}

/// Indents (or with Shift outdents) the selected lines, keeping them
/// selected.
fn indent_block(
    state: EditorState,
    text: String,
    (start, end): (TextSize, TextSize),
    outdent: bool,
    backward: bool,
) {
    let unit = state.doc().indent.get_value();
    let Some(indented) = indent_lines_through(&text, (start, end), 0, outdent, &unit) else {
        return;
    };
    let restore = if backward { (end, start) } else { (start, end) };
    let edit = selected_through(
        Edit::command(text, indented.change, restore),
        indented.selection,
        backward,
    );
    let (selection_start, selection_end) = edit.selected();
    let made = edit.before == edit.text || state.commit(edit);
    // Keep the block selected, in the same direction (a refused edit
    // left the selection it started from).
    if made && let Some(element) = state.textarea() {
        select(&element, selection_start, selection_end, backward);
    }
    state.close_suggestions();
}

/// `edit` leaving `selection` (bytes of the text after) selected, its
/// caret at the start with `backward` (the end that moves).
pub(super) fn selected_through(
    edit: Edit,
    (start, end): (TextSize, TextSize),
    backward: bool,
) -> Edit {
    if backward {
        edit.selecting(end, start)
    } else {
        edit.selecting(start, end)
    }
}
