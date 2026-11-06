//! Tab and Shift+Tab: placeholder slots, block indent, and tab stops.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{indent_lines, line_range, tab_insert};
use super::super::completion::next_slot;
use super::super::keyboard::{replace_selection, utf16_to_byte_index};
use super::super::state::{Edit, EditorState, is_backward, select, textarea_selection};

pub(super) fn tab(state: EditorState, event: &KeyboardEvent) {
    event.prevent_default();
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let text = state.source.get_untracked();
    if !event.shift_key()
        && start == end
        && (next_placeholder_slot(state, &text, start) || step_out_of_value(state, &text, start))
    {
        return;
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
    let (updated, caret) = replace_selection(
        &text,
        start,
        end,
        &tab_insert(&text, start, &state.indent.get_value()),
    );
    state.commit(Edit::new(text, (start, end), updated, caret));
    state.close_suggestions();
}

/// In a block a suggestion just wrote, Tab goes to its next empty value, as
/// through a snippet's placeholders.
fn next_placeholder_slot(state: EditorState, text: &str, caret: usize) -> bool {
    let Some((region_start, from_end)) = state.slot_region.get_value() else {
        return false;
    };
    let region_end = text.encode_utf16().count().saturating_sub(from_end);
    let slot = (caret >= region_start && caret <= region_end)
        .then(|| next_slot(text, caret, region_end, state.behavior()))
        .flatten();
    match slot {
        Some(slot) => {
            state.place_caret(slot);
            true
        }
        None => {
            state.slot_region.set_value(None);
            false
        }
    }
}

/// After a value typed before a closer the editor wrote (`"k.pem|"`,
/// `<listen>7836|</listen>`), Tab with no slot left steps past the closer,
/// as at the end of a snippet, instead of indenting inside the value.
fn step_out_of_value(state: EditorState, text: &str, caret: usize) -> bool {
    let Some(closer) = state.closer_at(caret) else {
        return false;
    };
    let Some(opener) = closer.opener else {
        return false;
    };
    if text[..utf16_to_byte_index(text, caret)].ends_with(opener) {
        return false;
    }
    state.forget_closer_at(caret);
    state.place_caret(caret + closer.len);
    true
}

/// Whether Tab indents the selected lines rather than replacing the
/// selection: it spans lines, or covers one whole line (as in VS Code).
fn spans_lines(text: &str, start: usize, end: usize) -> bool {
    if start == end {
        return false;
    }
    let (line_start, line_end) = line_range(text, start);
    let content_end = line_end
        - usize::from(text.encode_utf16().nth(line_end.wrapping_sub(1)) == Some(u16::from(b'\n')));
    let indent_end = line_start
        + text[utf16_to_byte_index(text, line_start)..]
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .count();
    let whole_line = start <= indent_end && end >= content_end;
    whole_line
        || text
            .encode_utf16()
            .skip(start)
            .take(end - start)
            .any(|unit| unit == u16::from(b'\n'))
}

/// Indents (or with Shift outdents) the selected lines, keeping them
/// selected.
fn indent_block(
    state: EditorState,
    text: String,
    (start, end): (usize, usize),
    outdent: bool,
    backward: bool,
) {
    let Some((updated, selection_start, selection_end)) =
        indent_lines(&text, start, end, outdent, &state.indent.get_value())
    else {
        return;
    };
    // The caret is the end that moves: the start of a backward selection.
    let (anchor, caret) = if backward {
        (selection_end, selection_start)
    } else {
        (selection_start, selection_end)
    };
    if updated != text {
        state.commit(Edit::new(text, (start, end), updated, caret).selecting_from(anchor));
    } else {
        state.note_caret(caret);
    }
    // Keep the block selected, in the same direction.
    if let Some(element) = state.textarea() {
        select(&element, selection_start, selection_end, backward);
    }
    state.close_suggestions();
}
