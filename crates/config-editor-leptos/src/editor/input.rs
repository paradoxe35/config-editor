//! Textarea events other than keydown: typing, clicks, caret moves,
//! scrolling, hover docs, and the whole-line clipboard.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::commands::line_range;
use super::completion::{key_hover, should_auto_trigger};
use super::components::HoverCard;
use super::edit::Edit;
use super::history::EditHistory;
use super::keyboard::replace_selection;
use super::positioning::{hover_card_style, pointer_text_position, schedule_hover};
use super::source_area::SourceArea;
use super::state::{EditorState, textarea_selection};
use crate::reactive::later;

/// The textarea an event came from, in document offsets.
fn target_textarea(state: EditorState, event: &web_sys::Event) -> Option<SourceArea> {
    let element: web_sys::HtmlTextAreaElement = event.target()?.dyn_into().ok()?;
    Some(SourceArea::new(element, state))
}

/// Read-only cancels edits here instead of setting `readonly`, which in
/// Chromium also stops the caret: navigation and selection keep working.
///
/// An edit the keys did not already open folds for (an IME, a drop, the
/// context menu) that would reach into a fold opens it and is cancelled:
/// the browser would apply it to the textarea's text, where the fold's
/// lines are missing.
pub(super) fn before_input(state: EditorState, event: &web_sys::InputEvent) {
    if state.config().read_only.get_untracked() {
        event.prevent_default();
        return;
    }
    if let Some(element) = target_textarea(state, event) {
        let (start, end) = textarea_selection(&element);
        let reach = match event.input_type().as_str() {
            kind if kind.starts_with("deleteContentBackward")
                || kind.starts_with("deleteWordBackward")
                || kind.starts_with("deleteSoftLineBackward")
                || kind.starts_with("deleteHardLineBackward") =>
            {
                (start.saturating_sub(usize::from(start == end)), end)
            }
            kind if kind.starts_with("delete") => (start, end + usize::from(start == end)),
            _ => (start, end),
        };
        if state.touches_fold(reach.0, reach.1) {
            event.prevent_default();
            state.unfold_touched(reach.0, reach.1);
            return;
        }
        state.typing().pre_edit_selection.set_value((start, end));
    }
}

/// The input events of one burst: the browser can send many in one task
/// (`execCommand("insertText")` with line breaks, dictation, some IMEs send
/// one per line). They are applied once, at the end of the task.
pub(super) struct InputBurst {
    /// The document and selection before the first event.
    before: String,
    selection: (usize, usize),
    /// The first event's type, and how many events came.
    input_type: String,
    events: usize,
    time: f64,
    /// The text and selection the browser left after the latest event:
    /// what the input made, even if something rewrites the textarea before
    /// the burst is applied.
    latest: String,
    latest_selection: (usize, usize),
}

/// No selection recorded before the input (see [`changed_range`]).
pub(super) const UNKNOWN_SELECTION: (usize, usize) = (usize::MAX, usize::MAX);

/// The UTF-16 range of `before` that the edit to `after` replaced: what was
/// selected (or the caret) when the edit was made.
pub(super) fn changed_range(before: &str, after: &str) -> (usize, usize) {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (prefix, old.len() - suffix)
}

/// The UTF-16 range of `before` the edit to `after` replaced, placed by the
/// caret it left (`caret`): inserted text ends there, which tells apart
/// places the texts alone cannot (`aa|a` + `a`).
pub(super) fn edit_range(before: &str, after: &str, caret: usize) -> (usize, usize) {
    let (start, end) = changed_range(before, after);
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let removed = end - start;
    let Some(inserted) = (new.len() + removed).checked_sub(old.len()) else {
        return (start, end);
    };
    if let Some(from) = caret.checked_sub(inserted)
        && caret <= new.len()
        && from + removed <= old.len()
        && old[..from] == new[..from]
        && old[from + removed..] == new[caret..]
    {
        return (from, from + removed);
    }
    (start, end)
}

/// The edit that turned `before` into `latest`, made on `current` instead
/// (where `before` became `current` meanwhile), and the caret after it.
pub(super) fn replay(before: &str, latest: &str, current: &str) -> (String, usize) {
    use super::sync::map_through_change;
    let (start, end) = changed_range(before, latest);
    let latest_units: Vec<u16> = latest.encode_utf16().collect();
    let before_len = before.encode_utf16().count();
    let inserted_len = latest_units.len() + (end - start) - before_len;
    let inserted = String::from_utf16_lossy(&latest_units[start..start + inserted_len]);
    let (from, to) = (
        map_through_change(before, current, start),
        map_through_change(before, current, end),
    );
    let current_units: Vec<u16> = current.encode_utf16().collect();
    let (from, to) = (
        from.min(current_units.len()),
        to.min(current_units.len())
            .max(from.min(current_units.len())),
    );
    let mut result = String::from_utf16_lossy(&current_units[..from]);
    result.push_str(&inserted);
    result.push_str(&String::from_utf16_lossy(&current_units[to..]));
    (result, from + inserted_len)
}

/// An edit the browser applied (typing, paste, cut, drop, IME).
pub(super) fn input(state: EditorState, event: &web_sys::Event) {
    if state.config().read_only.get_untracked() {
        state.show_text();
        return;
    }
    let input_type = event
        .dyn_ref::<web_sys::InputEvent>()
        .map(web_sys::InputEvent::input_type)
        .unwrap_or_default();
    let (latest, latest_selection) = target_textarea(state, event)
        .map(|element| (element.value(), textarea_selection(&element)))
        .unwrap_or_else(|| (state.doc().text.get_untracked(), (0, 0)));
    if state.typing().input_burst.with_value(Option::is_some) {
        state.typing().input_burst.update_value(|burst| {
            if let Some(burst) = burst {
                burst.events += 1;
                burst.latest = latest;
                burst.latest_selection = latest_selection;
            }
        });
        return;
    }
    let before = state.doc().text.get_untracked();
    // Without a `beforeinput` (`execCommand`, some synthetic events) the
    // selection is read off the change when the burst is applied.
    let selection = state.typing().pre_edit_selection.get_value();
    state
        .typing()
        .pre_edit_selection
        .set_value(UNKNOWN_SELECTION);
    state.typing().input_burst.set_value(Some(InputBurst {
        before,
        selection,
        input_type,
        events: 1,
        time: event.time_stamp(),
        latest,
        latest_selection,
    }));
    // Skipped if the editor unmounts first; a flush another editor or the
    // host starts drops it then.
    PENDING.with(|pending| {
        pending
            .borrow_mut()
            .push(later::guarded(state, flush_input))
    });
    later::soon(state, |_| flush_pending_input());
}

thread_local! {
    /// Editors holding input not applied yet (see [`flush_pending_input`]).
    static PENDING: std::cell::RefCell<Vec<Box<dyn FnOnce()>>> = std::cell::RefCell::new(Vec::new());
}

/// Applies input the browser sent in the current task that the editors have
/// not applied yet (they apply a burst of input events once, at the end of
/// the task). Call it before reading or replacing a document from code that
/// runs in the same task as such input, outside the editor's own handlers.
pub fn flush_pending_input() {
    let pending = PENDING.with(|pending| std::mem::take(&mut *pending.borrow_mut()));
    for flush in pending {
        flush();
    }
}

/// Applies the pending input burst, if any: one document change, one
/// `onChange`, one undo step.
pub(super) fn flush_input(state: EditorState) {
    let Some(Some(burst)) = state.typing().input_burst.try_update_value(Option::take) else {
        return;
    };
    if state.textarea().is_none() {
        return;
    }
    // What the input made, on the document as it is now: a host value
    // applied since the input (a `setValue` in the same task) gets the input
    // replayed on it, and a textarea rewritten meanwhile gets it back.
    let before = state.doc().text.get_untracked();
    let (value, replayed_caret) = if before == burst.before {
        (burst.latest, None)
    } else {
        let (value, caret) = replay(&burst.before, &burst.latest, &before);
        (value, Some(caret))
    };
    let selection = if replayed_caret.is_some() {
        UNKNOWN_SELECTION
    } else {
        burst.selection
    };
    // Several events at once are text arriving whole, as a paste does.
    let input_type = if burst.events > 1 {
        "insertFromPaste".to_string()
    } else {
        burst.input_type
    };
    // Backspace with the list open refilters it instead of closing.
    let refilter = input_type == "deleteContentBackward"
        && state.completion().requested.get_untracked()
        && !state.completion().suggestions.with_untracked(Vec::is_empty);
    let typed = replayed_caret.map_or(burst.latest_selection, |caret| (caret, caret));
    let edit = Edit::input(
        before.clone(),
        selection,
        value,
        typed,
        input_type.clone(),
        burst.time,
    );
    if !state.commit(edit) {
        return;
    }
    // The text as made (widened over a placeholder it cut).
    let value = state.doc().text.get_untracked();
    let auto_trigger = state.config().settings.auto_trigger.get_untracked()
        && should_auto_trigger(&input_type, &before, &value);
    if matches!(
        input_type.as_str(),
        "insertFromPaste" | "insertFromDrop" | "deleteByCut" | "deleteByDrag"
    ) {
        state.typing().closers.update_value(Vec::clear);
    }
    state.completion().placeholder.set_value(None);
    state.completion().active.set(0);
    // Typed text filters the list again.
    if state.completion().replace_word.get_untracked() {
        state.completion().replace_word.set(false);
    }
    if !refilter {
        state.completion().explicit.set(false);
    }
    state.completion().requested.set(auto_trigger || refilter);
    state.hide_hover();
    state.hide_shown_problem();
}

/// Hovering a key shows its schema docs after a short pause; hovering a
/// read-only value says why it is read-only.
pub(super) fn mouse_move(state: EditorState, event: &web_sys::MouseEvent) {
    let Some(element) = target_textarea(state, event) else {
        return;
    };
    if !state.completion().suggestions.with_untracked(Vec::is_empty) {
        state.hide_hover();
        return;
    }
    let (line, column) = pointer_text_position(
        &element,
        f64::from(event.client_x()),
        f64::from(event.client_y()),
    );
    let on_protected = state.hover_protected(line, column);
    if !state.config().settings.hover_docs.get_untracked() {
        return;
    }
    let info = state.doc().text.with_untracked(|text| {
        state
            .config()
            .schema
            .with_value(|schema| key_hover(text, line, column, schema.as_deref(), state.behavior()))
    });
    let Some(info) = info else {
        state.leave_hover();
        return;
    };
    // The value's own message takes the place of its key's docs there.
    if on_protected && column >= info.start_column + info.key.chars().count() {
        state.leave_hover();
        return;
    }
    let identity = (line, info.start_column, info.key.clone());
    if state.hover().pending.get_value().as_ref() == Some(&identity) {
        return;
    }
    let shown = state.hover().card.with_untracked(|card| {
        card.as_ref().is_some_and(|card| {
            card.line == line
                && card.info.start_column == info.start_column
                && card.info.key == info.key
        })
    });
    if shown {
        return;
    }
    state.hide_hover();
    state.hover().pending.set_value(Some(identity));
    let token = state.hover().token.get_value();
    let style = hover_card_style(&element, line, info.start_column);
    schedule_hover(state, move |state| {
        if state.hover().token.get_value() == token {
            state.hover().pending.set_value(None);
            state
                .hover()
                .card
                .set(Some(HoverCard { info, line, style }));
        }
    });
}

pub(super) fn blur(state: EditorState) {
    state.close_suggestions();
    state.hide_hover();
    state.protect().hide_tip(true);
    state.forget_refusal();
}

pub(super) fn click(state: EditorState, event: &web_sys::MouseEvent) {
    state.protect().hide_tip(false);
    state.forget_refusal();
    // The caret never lands inside a secret placeholder.
    state.snap_to_tokens();
    state.forget_typing_aids();
    state.hide_shown_problem();
    if let Some(element) = target_textarea(state, event) {
        let (position, _) = textarea_selection(&element);
        state.note_caret(position);
        state.completion().active.set(0);
        state.close_suggestions();
        state.hide_hover();
        if from_touch(event) {
            super::a11y::show_at_caret(state);
        }
    }
}

/// A mouse press in the text: where it landed, for a drag that starts on a
/// secret placeholder.
fn mouse_down(state: EditorState, event: &web_sys::MouseEvent) {
    if event.button() != 0 {
        return;
    }
    let Some(element) = target_textarea(state, event) else {
        return;
    };
    let (line, column) = pointer_text_position(
        &element,
        f64::from(event.client_x()),
        f64::from(event.client_y()),
    );
    let offset = state.doc().text.with_untracked(|text| {
        super::paths::TextPosition::new(line + 1, column + 1).to_offset(text)
    });
    state.press_at(offset);
}

/// The end of a mouse selection: a drag that started on a secret
/// placeholder takes it whole, anchored at its far edge (no `click`
/// follows a drag that ends off the text it started on).
fn mouse_up(state: EditorState) {
    state.release_press();
}

/// Whether a click was a tap on a touch screen.
fn from_touch(event: &web_sys::MouseEvent) -> bool {
    web_sys::js_sys::Reflect::get(event, &"pointerType".into())
        .ok()
        .and_then(|kind| kind.as_string())
        .is_some_and(|kind| kind == "touch")
}

/// The selection changed without a key or a click: a phone's long press
/// or selection handles, or a script. The caret follows it (the line
/// highlight, the caret's note, `on_path_change`).
///
/// It also comes between an arrow key's keydown (which moved the caret)
/// and its keyup, or between the keydowns of a held key: a list asked for
/// by typing that matched nothing ends there, before the new caret
/// position could open it (the key would then move through that list).
fn selection_change(state: EditorState) {
    if state.typing().input_burst.with_value(Option::is_some) {
        return;
    }
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, _) = textarea_selection(&element);
    if start != state.doc().caret.get_untracked() {
        if state.completion().requested.get_untracked()
            && state.completion().suggestions.with_untracked(Vec::is_empty)
        {
            state.close_suggestions();
            state.completion().explicit.set(false);
        }
        state.note_caret(start);
    }
}

/// After a key: track the caret, and end suggestions on navigation or a
/// selection.
pub(super) fn key_up(state: EditorState, event: &web_sys::KeyboardEvent) {
    let Some(element) = target_textarea(state, event) else {
        return;
    };
    state.snap_to_tokens();
    let (start, end) = textarea_selection(&element);
    // Whether the list was open when the key moved the caret: ArrowUp/Down
    // with it closed are caret navigation, even when the new caret position
    // would have suggestions.
    let list_was_open = !state.completion().suggestions.with_untracked(Vec::is_empty);
    // A selection (Ctrl+A, Shift+arrows) ends any suggestion list.
    if start != end && state.completion().requested.get_untracked() {
        state.close_suggestions();
    }
    if start != state.doc().caret.get_untracked() {
        state.note_caret(start);
    }
    // Caret navigation never shows suggestions. ArrowUp/Down move inside an
    // open list instead (handled on keydown).
    let navigation = match event.key().as_str() {
        "ArrowLeft" | "ArrowRight" | "Home" | "End" => true,
        "ArrowUp" | "ArrowDown" | "PageUp" | "PageDown" => !list_was_open,
        _ => false,
    };
    if navigation {
        state.forget_typing_aids();
        if state.completion().requested.get_untracked() {
            state.close_suggestions();
            state.completion().explicit.set(false);
            state.completion().active.set(0);
        }
    }
}

pub(super) fn scroll(state: EditorState, event: &web_sys::Event) {
    state.hide_hover();
    if let Some(element) = target_textarea(state, event) {
        state.view().scroll_x.set(f64::from(element.scroll_left()));
        state.view().scroll_y.set(f64::from(element.scroll_top()));
    }
}

/// Clipboard and composition listeners, added here rather than in the view:
/// a tachys element holds at most 26 attributes.
pub(super) fn attach_listeners(state: EditorState) {
    Effect::new(move |_| {
        use wasm_bindgen::closure::Closure;
        let Some(element) = state.view().textarea_ref.get() else {
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
                "selectionchange",
                clipboard(Box::new(move |_| selection_change(state))),
            ),
            // Each listener's event is cast to what it is (the closures
            // share one type).
            (
                "mousedown",
                clipboard(Box::new(move |event| {
                    mouse_down(state, event.unchecked_ref::<web_sys::MouseEvent>());
                })),
            ),
            ("mouseup", clipboard(Box::new(move |_| mouse_up(state)))),
            (
                "compositionend",
                clipboard(Box::new(move |_| {
                    // The composition's last input first, then the seal.
                    flush_input(state);
                    state.doc().history.update(EditHistory::seal);
                })),
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
    // A selection cutting a secret placeholder copies it whole.
    state.snap_to_tokens();
    let (start, end) = textarea_selection(&element);
    if start != end {
        copy_across_folds(state, event, (start, end), cut);
        return;
    }
    let Some(clipboard) = event.clipboard_data() else {
        return;
    };
    let text = state.doc().text.get_untracked();
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
    state.typing().copied_line.set_value(Some(line));
    if !cut || state.config().read_only.get_untracked() {
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
    state.commit(Edit::new(
        text,
        (start, end),
        updated,
        caret.max(next_line.0),
    ));
}

/// Copy or cut of a selection across a fold takes the hidden lines too:
/// the textarea's own copy would miss them.
fn copy_across_folds(
    state: EditorState,
    event: &web_sys::ClipboardEvent,
    (start, end): (usize, usize),
    cut: bool,
) {
    if !state
        .view()
        .projection
        .with_untracked(|projection| projection.hides_within(start, end))
    {
        return;
    }
    let Some(clipboard) = event.clipboard_data() else {
        return;
    };
    let text = state.doc().text.get_untracked();
    let units: Vec<u16> = text.encode_utf16().collect();
    let selected = String::from_utf16_lossy(&units[start.min(units.len())..end.min(units.len())]);
    if clipboard.set_data("text/plain", &selected).is_err() {
        return;
    }
    event.prevent_default();
    if !cut || state.config().read_only.get_untracked() {
        return;
    }
    let (updated, _) = replace_selection(&text, start, end, "");
    state.commit(Edit::new(text, (start, end), updated, start));
}

/// Pasting a line copied with nothing selected puts it above the caret's
/// line and keeps the caret on its text, as in VS Code.
fn paste_line(state: EditorState, event: &web_sys::ClipboardEvent) {
    let (Some(element), Some(clipboard)) = (state.textarea(), event.clipboard_data()) else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    // The paste lands on the whole text: the folds it touches open first.
    if !state.config().read_only.get_untracked() {
        state.unfold_touched(start, end);
    }
    let Ok(pasted) = clipboard.get_data("text/plain") else {
        return;
    };
    if start != end
        || state.config().read_only.get_untracked()
        || state
            .typing()
            .copied_line
            .with_value(|line| line.as_deref() != Some(pasted.as_str()))
    {
        return;
    }
    event.prevent_default();
    let text = state.doc().text.get_untracked();
    let (line_start, _) = line_range(&text, start);
    let (updated, _) = replace_selection(&text, line_start, line_start, &pasted);
    let caret = start + pasted.encode_utf16().count();
    state.commit(Edit::new(text, (start, end), updated, caret));
}

#[cfg(test)]
mod tests {
    use super::changed_range;

    /// Round 11: the selection an edit replaced, read off the change when
    /// the browser sent no `beforeinput` (undo puts the caret back there).
    #[test]
    fn the_replaced_range_is_read_off_the_change() {
        assert_eq!(changed_range("abc", "abXc"), (2, 2));
        assert_eq!(changed_range("abc", "aXYc"), (1, 2));
        assert_eq!(changed_range("aaa", "aaaa"), (3, 3));
        assert_eq!(changed_range("a\u{1F600}b", "a\u{1F600}xb"), (3, 3));
        assert_eq!(changed_range("abc", "ac"), (1, 2));
        // Placed by the caret where the texts alone are ambiguous (round 12).
        use super::edit_range;
        assert_eq!(edit_range("a: 1\nb: 2", "a: 1\nx: 1\nb: 2", 9), (4, 4));
        assert_eq!(edit_range("k: aaa", "k: aaaa", 6), (5, 5));
        assert_eq!(edit_range("a: 1\na: 1", "a: 1\na: 1\na: 1", 10), (5, 5));
    }

    /// Round 11: input made before a host value was applied is made on that
    /// value instead of being lost.
    #[test]
    fn input_is_replayed_on_a_document_replaced_meanwhile() {
        use super::replay;
        assert_eq!(
            replay("listen: 1\n", "listen: 1\nQ", "x: 1\n"),
            ("x: 1\nQ".to_string(), 6)
        );
        assert_eq!(
            replay("a: 1\nb: 2\n", "a: 1\nb: 2X\n", "a: 1\nb: 2\nc: 3\n"),
            ("a: 1\nb: 2X\nc: 3\n".to_string(), 10)
        );
        assert_eq!(replay("abc", "aXYc", "abc"), ("aXYc".to_string(), 3));
    }
}
