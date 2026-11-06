//! Textarea events other than keydown: typing, clicks, caret moves,
//! scrolling, hover docs, and the whole-line clipboard.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::commands::{line_at_cursor, line_range};
use super::completion::{key_hover, should_auto_trigger};
use super::components::HoverCard;
use super::history::{EditHistory, Snapshot};
use super::keyboard::replace_selection;
use super::positioning::{hover_card_style, pointer_text_position, schedule_hover};
use super::state::{Edit, EditorState, textarea_selection};

fn target_textarea(event: &web_sys::Event) -> Option<web_sys::HtmlTextAreaElement> {
    event.target()?.dyn_into().ok()
}

/// Read-only cancels edits here instead of setting `readonly`, which in
/// Chromium also stops the caret: navigation and selection keep working.
pub(super) fn before_input(state: EditorState, event: &web_sys::InputEvent) {
    if state.read_only.get_untracked() {
        event.prevent_default();
        return;
    }
    if let Some(element) = target_textarea(event) {
        state
            .pre_edit_selection
            .set_value(textarea_selection(&element));
    }
}

/// An edit the browser applied (typing, paste, cut, drop, IME).
pub(super) fn input(state: EditorState, event: &web_sys::Event) {
    if state.read_only.get_untracked() {
        if let Some(element) = target_textarea(event) {
            element.set_value(&state.source.get_untracked());
        }
        return;
    }
    let value = event_target_value(event);
    let before = state.source.get_untracked();
    let input_type = event
        .dyn_ref::<web_sys::InputEvent>()
        .map(web_sys::InputEvent::input_type)
        .unwrap_or_default();
    let auto_trigger =
        state.features.auto_trigger && should_auto_trigger(&input_type, &before, &value);
    if matches!(
        input_type.as_str(),
        "insertFromPaste" | "insertFromDrop" | "deleteByCut" | "deleteByDrag"
    ) {
        state.auto_closers.update_value(Vec::clear);
    }
    // Backspace with the list open refilters it instead of closing.
    let refilter = input_type == "deleteContentBackward"
        && state.suggestions_requested.get_untracked()
        && !state.suggestions.with_untracked(Vec::is_empty);
    state.placeholder.set_value(None);
    let after = target_textarea(event).map(|element| textarea_selection(&element));
    if before != value {
        state.history.update(|history| {
            history.record_input(
                Snapshot {
                    value: before,
                    selection: state.pre_edit_selection.get_value(),
                },
                after.unwrap_or_default(),
                &input_type,
                event.time_stamp(),
            );
        });
    }
    if let Some((position, _)) = after {
        state.cursor.set(position);
        state.current_line.set(line_at_cursor(&value, position));
    }
    state.source.set(value);
    if let Some(on_change) = state.on_change {
        on_change.run(state.source.get_untracked());
    }
    state.active_completion.set(0);
    if !refilter {
        state.explicit_request.set(false);
    }
    state.suggestions_requested.set(auto_trigger || refilter);
    state.hide_hover();
}

/// Hovering a key shows its schema docs after a short pause.
pub(super) fn mouse_move(state: EditorState, event: &web_sys::MouseEvent) {
    if !state.features.hover_docs {
        return;
    }
    let Some(element) = target_textarea(event) else {
        return;
    };
    if !state.suggestions.with_untracked(Vec::is_empty) {
        state.hide_hover();
        return;
    }
    let (line, column) = pointer_text_position(
        &element,
        f64::from(event.client_x()),
        f64::from(event.client_y()),
    );
    let info = state.source.with_untracked(|text| {
        state
            .schema
            .with_value(|schema| key_hover(text, line, column, schema.as_deref(), state.behavior()))
    });
    let Some(info) = info else {
        state.hide_hover();
        return;
    };
    let identity = (line, info.key.clone());
    if state.hover_pending.get_value().as_ref() == Some(&identity) {
        return;
    }
    let shown = state.hover_card.with_untracked(|card| {
        card.as_ref()
            .is_some_and(|card| card.line == line && card.info.key == info.key)
    });
    if shown {
        return;
    }
    state.hide_hover();
    state.hover_pending.set_value(Some(identity));
    let token = state.hover_token.get_value();
    let style = hover_card_style(&element, line, info.start_column);
    schedule_hover(move || {
        // The editor may have been unmounted while this waited.
        if state.hover_token.try_get_value() == Some(token) {
            let _ = state.hover_pending.try_set_value(None);
            let _ = state
                .hover_card
                .try_set(Some(HoverCard { info, line, style }));
        }
    });
}

pub(super) fn blur(state: EditorState) {
    state.close_suggestions();
    state.hide_hover();
}

pub(super) fn click(state: EditorState, event: &web_sys::MouseEvent) {
    state.forget_typing_aids();
    if let Some(element) = target_textarea(event) {
        let (position, _) = textarea_selection(&element);
        state.note_caret(position);
        state.active_completion.set(0);
        state.close_suggestions();
        state.hide_hover();
    }
}

/// After a key: track the caret, and end suggestions on navigation or a
/// selection.
pub(super) fn key_up(state: EditorState, event: &web_sys::KeyboardEvent) {
    let Some(element) = target_textarea(event) else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    // A selection (Ctrl+A, Shift+arrows) ends any suggestion list.
    if start != end && state.suggestions_requested.get_untracked() {
        state.close_suggestions();
    }
    if start != state.cursor.get_untracked() {
        state.note_caret(start);
    }
    // Caret navigation never shows suggestions. ArrowUp/Down move inside an
    // open list instead (handled on keydown).
    let navigation = match event.key().as_str() {
        "ArrowLeft" | "ArrowRight" | "Home" | "End" => true,
        "ArrowUp" | "ArrowDown" | "PageUp" | "PageDown" => {
            state.suggestions.with_untracked(Vec::is_empty)
        }
        _ => false,
    };
    if navigation {
        state.forget_typing_aids();
        if state.suggestions_requested.get_untracked() {
            state.close_suggestions();
            state.explicit_request.set(false);
            state.active_completion.set(0);
        }
    }
}

pub(super) fn scroll(state: EditorState, event: &web_sys::Event) {
    state.hide_hover();
    if let Some(element) = target_textarea(event) {
        state.scroll_x.set(f64::from(element.scroll_left()));
        state.scroll_y.set(f64::from(element.scroll_top()));
    }
}

/// Clipboard and composition listeners, added here rather than in the view:
/// a tachys element holds at most 26 attributes.
pub(super) fn attach_listeners(state: EditorState) {
    Effect::new(move |_| {
        use wasm_bindgen::closure::Closure;
        let Some(element) = state.editor_ref.get() else {
            return;
        };
        let clipboard = |handler: Box<dyn Fn(web_sys::ClipboardEvent)>| {
            Closure::<dyn Fn(web_sys::ClipboardEvent)>::wrap(handler)
        };
        let listeners = [
            (
                "copy",
                clipboard(Box::new(move |event| copy_line(state, &event, false))),
            ),
            (
                "cut",
                clipboard(Box::new(move |event| copy_line(state, &event, true))),
            ),
            (
                "paste",
                clipboard(Box::new(move |event| paste_line(state, &event))),
            ),
            (
                "compositionend",
                clipboard(Box::new(move |_| state.history.update(EditHistory::seal))),
            ),
        ];
        for (name, listener) in &listeners {
            let _ =
                element.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        // Browser objects stay on the one wasm thread.
        let registered = StoredValue::new_local((element, listeners));
        on_cleanup(move || {
            registered.with_value(|(element, listeners)| {
                for (name, listener) in listeners {
                    let _ = element.remove_event_listener_with_callback(
                        name,
                        listener.as_ref().unchecked_ref(),
                    );
                }
            });
        });
    });
}

/// Copy or cut with nothing selected takes the whole line, as in VS Code.
fn copy_line(state: EditorState, event: &web_sys::ClipboardEvent, cut: bool) {
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let (Some(clipboard), true) = (event.clipboard_data(), start == end) else {
        return;
    };
    let text = state.source.get_untracked();
    let (line_start, line_end) = line_range(&text, start);
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut line = String::from_utf16_lossy(&units[line_start..line_end]);
    if !line.ends_with('\n') {
        line.push('\n');
    }
    if clipboard.set_data("text/plain", &line).is_err() {
        return;
    }
    event.prevent_default();
    state.copied_line.set_value(Some(line));
    if !cut || state.read_only.get_untracked() {
        return;
    }
    // The last line takes the newline before it, leaving no empty line.
    let removed_start = if line_end == units.len()
        && units.get(line_end.wrapping_sub(1)) != Some(&u16::from(b'\n'))
    {
        line_start.saturating_sub(1)
    } else {
        line_start
    };
    let (updated, _) = replace_selection(&text, removed_start, line_end, "");
    // The caret keeps its column on the line that moves up.
    let column = start - line_start;
    let next_line = line_range(&updated, removed_start.min(updated.encode_utf16().count()));
    let ends_with_newline =
        updated.encode_utf16().nth(next_line.1.wrapping_sub(1)) == Some(u16::from(b'\n'));
    let caret = (next_line.0 + column).min(next_line.1 - usize::from(ends_with_newline));
    state.commit(Edit::new(text, (start, end), updated, caret.max(next_line.0)).revealed());
}

/// Pasting a line copied with nothing selected puts it above the caret's
/// line and keeps the caret on its text, as in VS Code.
fn paste_line(state: EditorState, event: &web_sys::ClipboardEvent) {
    let (Some(element), Some(clipboard)) = (state.textarea(), event.clipboard_data()) else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let Ok(pasted) = clipboard.get_data("text/plain") else {
        return;
    };
    if start != end
        || state.read_only.get_untracked()
        || state
            .copied_line
            .with_value(|line| line.as_deref() != Some(pasted.as_str()))
    {
        return;
    }
    event.prevent_default();
    let text = state.source.get_untracked();
    let (line_start, _) = line_range(&text, start);
    let (updated, _) = replace_selection(&text, line_start, line_start, &pasted);
    let caret = start + pasted.encode_utf16().count();
    state.commit(Edit::new(text, (start, end), updated, caret).revealed());
}
