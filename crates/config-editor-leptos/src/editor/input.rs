//! Textarea events other than keydown: typing, clicks, caret moves,
//! scrolling, hover docs, and the whole-line clipboard.

use leptos::prelude::*;
use unicode_segmentation::GraphemeCursor;
use wasm_bindgen::JsCast;

use super::commands::{copied_lines, line_range};
use super::completion::should_auto_trigger;
use super::edit::{Assoc, Change, Edit};
use super::history::{EditHistory, Selection};
use super::keyboard::utf16_to_byte_index;
use super::positioning::pointer_text_position;
use super::problem_hit::column_at_cell;
use super::source_area::SourceArea;
use super::state::{EditorState, directed_selection, normalize_newlines, textarea_selection};
use crate::editor::text::{TextRange, TextSize, Utf16, byte_of, slice, utf16_of};
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
///
/// Text of several lines arriving whole in one command (an IME commit, a
/// dictation, `Input.insertText`) is applied here, as a paste is, and the
/// browser's own insert cancelled: Chrome's textarea inserts it line by
/// line, in time growing with the lines inserted times the lines there
/// (six seconds for 400 lines into 4500, in a bare textarea too).
pub(super) fn before_input(state: EditorState, event: &web_sys::InputEvent) {
    if state.config().read_only.get_untracked() {
        event.prevent_default();
        state.note_read_only();
        return;
    }
    if let Some(element) = target_textarea(state, event) {
        let (start, end) = textarea_selection(&element);
        let kind = event.input_type();
        if kind == "deleteByDrag" {
            state.drag_starts(start, end);
            // The document was rewritten under the drag: refused, its drop
            // with it.
            if state.rewritten_under_drag() {
                event.prevent_default();
                return;
            }
            // Taking a closed fold's shown closing line away rewrites the
            // textarea under the drag: refused, its drop with it.
            if state.delete_reshapes_folds(start, end) {
                event.prevent_default();
                return;
            }
            // Part of a placeholder dragged: refused, its drop with it.
            if state.cuts_a_token(start, end) {
                event.prevent_default();
                return;
            }
        } else if kind == "insertFromDrop" && state.drop_refused_early() {
            event.prevent_default();
            state.drop_done();
            return;
        }
        // A deletion with nothing selected reaches over the character
        // beside the caret.
        let reach = state.doc().text.with_untracked(|text| match kind.as_str() {
            kind if start == end
                && (kind.starts_with("deleteContentBackward")
                    || kind.starts_with("deleteWordBackward")
                    || kind.starts_with("deleteSoftLineBackward")
                    || kind.starts_with("deleteHardLineBackward")) =>
            {
                (slice::step_back(text, start), end)
            }
            kind if start == end && kind.starts_with("delete") => {
                (start, slice::step_forward(text, end))
            }
            _ => (start, end),
        });
        if state.touches_fold(reach.0, reach.1) {
            event.prevent_default();
            if kind == "insertFromDrop" {
                state.drop_refused();
            }
            state.unfold_touched(reach.0, reach.1);
            return;
        }
        if let Some(text) = whole_lines(event) {
            event.prevent_default();
            insert_whole(state, (start, end), text, event.time_stamp());
            return;
        }
        // As (anchor, caret): undo puts a backward selection back backward.
        state
            .typing()
            .pre_edit_selection
            .set_value(directed_selection(&element));
    }
}

/// The text of several lines `event` inserts in one command, as the
/// textarea would hold it (line breaks as LF); `None` for any other input
/// (typing, a composition still going on: its events cannot be cancelled).
fn whole_lines(event: &web_sys::InputEvent) -> Option<String> {
    if event.input_type() != "insertText" || event.is_composing() {
        return None;
    }
    event
        .data()
        .filter(|text| text.contains(['\n', '\r']))
        .map(normalize_newlines)
}

/// Puts `text` over the document's `selection` (bytes) as a paste does:
/// one change, one undo step.
fn insert_whole(state: EditorState, selection: Selection, text: String, time: f64) {
    // Input still pending in this task first, so this lands after it.
    flush_input(state);
    let before = state.doc().text.get_untracked();
    let range = TextRange::new(selection.0, selection.1.max(selection.0));
    let change = Change::replace(&before, range, &text);
    let kind = "insertFromPaste";
    // The textarea still has the text before: written, caret in view.
    let edit = Edit::command(before.clone(), change, selection).typed(kind, time);
    apply_input(state, edit, &before, kind);
}

/// The input events of one burst: the browser can send many in one task
/// (`execCommand("insertText")` with line breaks, dictation, some IMEs send
/// one per line). They are applied once, at the end of the task.
pub(super) struct InputBurst {
    /// The document, its version and the selection (anchor, caret) before
    /// the first event.
    before: String,
    version: u64,
    selection: Selection,
    /// The first event's type, and how many events came.
    input_type: String,
    events: usize,
    time: f64,
    /// The text and selection the browser left after the latest event:
    /// what the input made, even if something rewrites the textarea before
    /// the burst is applied.
    latest: String,
    latest_selection: Selection,
}

/// No selection recorded before the input: the browser sent no
/// `beforeinput` (`execCommand`, a synthetic event), so its change is read
/// off the texts (see `edit::Edit::input`).
pub(super) const UNKNOWN_SELECTION: Selection = (TextSize::new(u32::MAX), TextSize::new(u32::MAX));

/// The input `mine` (made on `before`) made on `current` instead, where the
/// host made `before` into `current` meanwhile: rebased over the host's
/// change (read off the two texts, the host's), the input's text first
/// where both wrote at one place.
pub(super) fn replay(mine: &Change, before: &str, current: &str) -> Option<Change> {
    mine.map(&Change::diffed(before, current), true)
}

/// An edit the browser applied (typing, paste, cut, drop, IME).
pub(super) fn input(state: EditorState, event: &web_sys::Event) {
    if state.config().read_only.get_untracked() {
        state.show_text();
        return;
    }
    let input_event = event.dyn_ref::<web_sys::InputEvent>();
    let input_type = input_event
        .map(web_sys::InputEvent::input_type)
        .unwrap_or_default();
    if state.typing().composing.get_value()
        && !input_event.is_some_and(web_sys::InputEvent::is_composing)
    {
        end_composition_unannounced(state);
    }
    // The selection the browser left is in the text it left, not yet the
    // document's.
    let (latest, latest_selection) = target_textarea(state, event)
        .map(|element| {
            let latest = element.value();
            let selection = element.selection_in(&latest);
            (latest, selection)
        })
        .unwrap_or_else(|| {
            let none = TextSize::default();
            (state.doc().text.get_untracked(), (none, none))
        });
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
    let version = state.doc().version.get_value();
    // Without a `beforeinput` (`execCommand`, some synthetic events) the
    // change is read off the texts when the burst is applied.
    let selection = state.typing().pre_edit_selection.get_value();
    state
        .typing()
        .pre_edit_selection
        .set_value(UNKNOWN_SELECTION);
    state.typing().input_burst.set_value(Some(InputBurst {
        before,
        version,
        selection,
        input_type,
        events: 1,
        time: event.time_stamp(),
        latest,
        latest_selection,
    }));
    // While an input method composes, its updates stay in the textarea: the
    // composition is applied once, when it ends (each update written back
    // into the textarea mid-composition cost the browser seconds on a long
    // text, and a widened update was applied twice). Code that needs the
    // text before then flushes it itself.
    if state.typing().composing.get_value() {
        return;
    }
    // Skipped if the editor unmounts first; a flush another editor or the
    // host starts drops it then.
    PENDING.with(|pending| {
        pending
            .borrow_mut()
            .push(later::guarded(state, flush_input))
    });
    later::soon(state, |_| flush_pending_input());
}

/// The composition is over without a `compositionend`: an input that is
/// not part of it came. Chrome ends a composition this way when the
/// textarea's text is rewritten under it (a host value arriving while the
/// user composes), and the commit then arrives as plain `insertText`. The
/// composed text not committed went with the rewrite, so the input held
/// for the composition is dropped: what follows is typing on the text the
/// textarea shows, and is applied as such (P4: the commit is never held
/// back waiting for an end that does not come).
fn end_composition_unannounced(state: EditorState) {
    state.typing().composing.set_value(false);
    state.typing().input_burst.set_value(None);
    state.doc().history.update(EditHistory::seal);
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
    // Several events at once are text arriving whole, as a paste does.
    let input_type = if burst.events > 1 {
        "insertFromPaste".to_string()
    } else {
        burst.input_type
    };
    let before = state.doc().text.get_untracked();
    // Nothing to apply: the browser sends text arriving whole in one
    // command (an IME commit, a drop, `Input.insertText`) as one input event
    // per line, each in its own microtask, with the whole text already in
    // the textarea at the first; the first applies it (or is refused, and
    // the textarea shows the document again), the rest find nothing new.
    // Committing each again would redo every per-text step once per line.
    if burst.latest == before {
        return;
    }
    let input = Edit::input(
        burst.before.clone(),
        burst.selection,
        burst.latest,
        burst.latest_selection,
        input_type.clone(),
        burst.time,
    );
    // What the input made, on the document as it is now: a host value
    // applied since the input (a `setValue` in the same task) gets the input
    // replayed on it, and a textarea rewritten meanwhile gets it back.
    let edit = if state.doc().version.get_value() == burst.version {
        input
    } else {
        let Some(change) = replay(input.made(), &burst.before, &before) else {
            return;
        };
        let told = input.told();
        Edit::replayed(before.clone(), change, told, input_type.clone(), burst.time)
    };
    if edit.text == before {
        return;
    }
    apply_input(state, edit, &before, &input_type);
}

/// Commits `edit`, input the browser made (or the editor made for it) on
/// `before`, and updates the suggestions and popups after it.
pub(super) fn apply_input(state: EditorState, edit: Edit, before: &str, input_type: &str) {
    let showing = state.suggestions_showing();
    let mode = state.list_mode();
    // "No suggestions" is said only for a list the user asked for.
    let listed = showing && mode.heeded();
    // Backspace with the list asked for refilters it instead of closing,
    // also when the filter had left nothing: taking back what emptied it
    // brings the options back. A list the user closed is not asked for,
    // nor one typing asked for that showed no options since (its filter
    // emptied nothing, so a Backspace brings nothing back).
    let refilter = input_type == "deleteContentBackward" && mode.brings_back(showing);
    let committed = state.commit(edit);
    if input_type == "insertFromDrop" {
        if committed {
            state.drop_done();
        } else {
            state.drop_refused();
        }
    }
    if !committed {
        return;
    }
    // The text as made (widened over a placeholder it cut).
    let value = state.doc().text.get_untracked();
    let auto_trigger = state.config().settings.auto_trigger.get_untracked()
        && should_auto_trigger(input_type, before, &value);
    if matches!(
        input_type,
        "insertFromPaste" | "insertFromDrop" | "deleteByCut" | "deleteByDrag"
    ) {
        state.typing().closers.update_value(Vec::clear);
    }
    state.completion().placeholder.set_value(None);
    state.completion().active.set(0);
    // Typed text filters the list again.
    state.set_list_mode(mode.after_typing(auto_trigger, refilter, showing));
    super::a11y::say_no_suggestions_after(state, listed);
    if !state.suggestions_showing() {
        // A list that opens by itself is not one the user asked for.
        state.set_list_mode(state.list_mode().heeding(false));
    }
    state.hide_hover();
    state.hide_shown_problem();
}

/// The pointer left the text: the docs card and the problem tip close
/// (each after its moment to reach it).
pub(super) fn mouse_leave(state: EditorState, event: &web_sys::MouseEvent) {
    let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
    state.pointer_left_text(event.related_target().as_ref(), x, y);
}

/// The pointer moved over the gutter: as off the text, for the way to the
/// popup it can enter (see `popups::path`).
pub(super) fn gutter_move(state: EditorState, event: &web_sys::MouseEvent) {
    if event.buttons() == 0 {
        state.pointer_off_text(f64::from(event.client_x()), f64::from(event.client_y()));
    }
}

/// A press on the gutter (a line number, a marker, a fold control, the empty
/// space between): the focus stays in the text, or goes there when it was
/// elsewhere. The press is the start of a click on a line number or a marker
/// or of a drag from one: a drag ends over the text, and the keys typed after
/// it belong to the editor, not to the page the press would blur it to.
pub(super) fn gutter_press(state: EditorState, event: &web_sys::MouseEvent) {
    if event.button() != 0 {
        return;
    }
    event.prevent_default();
    if let Some(element) = state.textarea() {
        let _ = element.focus_in_place();
    }
}

/// Hovering a problem shows its tip (see `problem_hit`); hovering a key
/// shows its schema docs after a short pause; hovering a
/// read-only value says why it is read-only (see `popups`).
pub(super) fn mouse_move(state: EditorState, event: &web_sys::MouseEvent) {
    let Some(element) = target_textarea(state, event) else {
        return;
    };
    let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
    state.pointer_on_text();
    // Not while a button is held (a drag selecting text).
    state.pointer_at(&element, x, y, event.buttons() == 0);
}

pub(super) fn blur(state: EditorState, event: &web_sys::FocusEvent) {
    state.close_suggestions();
    if !state.keep_card_for(event.related_target().as_ref()) {
        state.hide_hover();
    }
    state.protect().hide_tip(true);
}

pub(super) fn click(state: EditorState, event: &web_sys::MouseEvent) {
    state.protect().hide_tip(false);
    // The caret never lands inside a secret placeholder.
    state.snap_to_tokens();
    state.forget_typing_aids();
    state.hide_shown_problem();
    if let Some(element) = target_textarea(state, event) {
        element.follow_caret();
        state.completion().active.set(0);
        state.close_suggestions();
        state.hide_hover();
        if from_touch(event) {
            super::a11y::show_at_caret(state);
        } else {
            // The pointer is still over the problems it clicked.
            let (line, cell) = pointer_text_position(
                &element,
                f64::from(event.client_x()),
                f64::from(event.client_y()),
            );
            state.point_at_problems(line, cell);
        }
    }
}

/// A mouse press in the text: where it landed, for a drag that starts on a
/// secret placeholder.
fn mouse_down(state: EditorState, event: &web_sys::MouseEvent) {
    if event.button() != 0 {
        return;
    }
    state.view().page_goal.set_value(None);
    // A press is no drag going on (one whose end never came).
    state.drag_ended();
    let Some(element) = target_textarea(state, event) else {
        return;
    };
    let (line, cell) = pointer_text_position(
        &element,
        f64::from(event.client_x()),
        f64::from(event.client_y()),
    );
    let offset = state
        .doc()
        .text
        .with_untracked(|text| press_offset(text, line, cell));
    state.press_at(offset);
}

/// The byte a press on rendered cell `cell` of zero-based `line` lands
/// on: the character drawn there (a tab is wider than a cell, a joined
/// accent takes none).
fn press_offset(text: &str, line: usize, cell: usize) -> TextSize {
    let column = column_at_cell(super::paths::split_line(text, line).unwrap_or(""), cell);
    let units = super::paths::TextPosition::new(line + 1, column).to_offset(text);
    byte_of(text, units)
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
    if element.caret() != state.doc().caret.get_untracked() {
        if state.completion().suggestions.with_untracked(Vec::is_empty) {
            state.close_suggestions();
        }
        element.follow_caret();
    }
}

/// After a key: track the caret, and end suggestions on navigation or a
/// selection.
pub(super) fn key_up(state: EditorState, event: &web_sys::KeyboardEvent) {
    // The input method's keys: the caret and selection are its own until
    // the composition ends.
    if crate::ime::composing(event) || state.typing().composing.get_value() {
        return;
    }
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
    if start != end {
        state.close_suggestions();
    }
    if element.caret() != state.doc().caret.get_untracked() {
        element.follow_caret();
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
        if state.list_mode().is_open() {
            state.close_suggestions();
            state.completion().active.set(0);
        }
    }
}

pub(super) fn scroll(state: EditorState, event: &web_sys::Event) {
    state.hide_hover();
    if let Some(element) = target_textarea(state, event) {
        element.mirror_scroll();
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
                "dragstart",
                clipboard(Box::new(move |_| state.drag_began())),
            ),
            ("dragend", clipboard(Box::new(move |_| state.drag_ended()))),
            (
                "compositionstart",
                clipboard(Box::new(move |_| state.typing().composing.set_value(true))),
            ),
            (
                "compositionend",
                clipboard(Box::new(move |_| {
                    state.typing().composing.set_value(false);
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
    // A selection cutting a secret placeholder copies it whole. The
    // browser's own copy takes the selection it had before the snap, so a
    // snapped selection is copied here. The copy is cut as the browser's
    // own is, in UTF-16 units: a selection may end inside a character.
    let selected_before = element.selection_utf16();
    state.snap_to_tokens();
    let (start_units, end_units) = element.selection_utf16();
    let (start, end) = textarea_selection(&element);
    if start != end {
        let (selected, split) = state.doc().text.with_untracked(|text| {
            let units: Vec<u16> = text.encode_utf16().collect();
            let (from, to) = (usize::from(start_units), usize::from(end_units));
            let selected =
                String::from_utf16_lossy(&units[from.min(units.len())..to.min(units.len())]);
            (selected, splits_a_character(text, (start_units, end_units)))
        });
        state.note_copied(&selected, (start, end));
        // The browser's own copy of a selection splitting a character
        // drawn as one writes nothing: the editor copies it.
        let ours = selected_before != (start_units, end_units) || split;
        copy_selection(state, event, (start_units, end_units), cut, ours);
        return;
    }
    let Some(clipboard) = event.clipboard_data() else {
        return;
    };
    let text = state.doc().text.get_untracked();
    let (line_start, line_end) = state.view().projection.with_untracked(|projection| {
        copied_lines(state.behavior(), &text, utf16_of(&text, start), projection)
    });
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut line = String::from_utf16_lossy(&units[line_start..line_end]);
    if !line.ends_with('\n') {
        line.push('\n');
    }
    if clipboard.set_data("text/plain", &line).is_err() {
        return;
    }
    event.prevent_default();
    state.note_copied(
        &line,
        (byte_of(&text, line_start), byte_of(&text, line_end)),
    );
    state.typing().copied_line.set_value(Some(line));
    // In a read-only editor the cut is refused, and says so.
    if !cut {
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
    let change = Change::replace_units(&text, removed_start, line_end, "");
    let column = utf16_of(&text, start) - line_start;
    let edit = Edit::command(text, change, (start, end));
    // The caret keeps its column on the line that moves up.
    let updated = &edit.text;
    let next_line = line_range(updated, removed_start.min(updated.encode_utf16().count()));
    let ends_with_newline =
        updated.encode_utf16().nth(next_line.1.wrapping_sub(1)) == Some(u16::from(b'\n'));
    let caret = (next_line.0 + column).min(next_line.1 - usize::from(ends_with_newline));
    let caret = byte_of(updated, caret.max(next_line.0));
    state.commit(edit.caret_at(caret));
}

/// Whether the selection `(start, end)` (UTF-16 units of `text`) has an
/// edge inside a character drawn as one: inside a surrogate pair, or
/// inside a grapheme (an emoji joined to others, a letter and its accent).
fn splits_a_character(text: &str, (start, end): (Utf16, Utf16)) -> bool {
    [start, end].into_iter().any(|edge| {
        let byte = utf16_to_byte_index(text, usize::from(edge));
        Utf16::of(&text[..byte]) != edge
            || GraphemeCursor::new(byte, text.len(), true).is_boundary(text, 0) != Ok(true)
    })
}

/// Copy or cut of a selection the textarea's own copy would get wrong:
/// one across a fold takes the hidden lines too, one snapped to a
/// placeholder's edges takes the placeholder whole, one splitting a
/// character is copied as selected; `ours` says it is one of the last two.
fn copy_selection(
    state: EditorState,
    event: &web_sys::ClipboardEvent,
    (start, end): (Utf16, Utf16),
    cut: bool,
    ours: bool,
) {
    let (start, end) = (usize::from(start), usize::from(end));
    if !ours
        && !state
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
    // In a read-only editor the cut is refused, and says so.
    if !cut {
        return;
    }
    let change = Change::replace_units(&text, start, end, "");
    let selection = (byte_of(&text, start), byte_of(&text, end));
    state.commit(Edit::command(text, change, selection));
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
    let (line_start, _) = line_range(&text, utf16_of(&text, start));
    let change = Change::insert_units(&text, line_start, &pasted);
    // The caret stays on its text, which the pasted line moves down.
    state.commit(Edit::command(text, change, (start, end)).keeping_caret(start, Assoc::After));
}

#[cfg(test)]
mod tests {
    use crate::editor::text::Utf16;

    /// Where a press lands, as a UTF-16 offset of `text`.
    fn press_units(text: &str, line: usize, cell: usize) -> usize {
        crate::editor::text::utf16_of(text, press_offset(text, line, cell))
    }

    /// A selection written as UTF-16 offsets.
    fn units(start: usize, end: usize) -> (Utf16, Utf16) {
        (Utf16::count(start), Utf16::count(end))
    }
    use super::super::edit::Change;
    use super::{press_offset, replay, splits_a_character};

    /// A selection edge inside a character drawn as one (half a joined
    /// emoji, a letter without its accent, half a surrogate pair) splits
    /// it: the textarea's own copy of such a selection writes nothing.
    #[test]
    fn a_selection_inside_a_joined_character_splits_it() {
        // `é`, then a man and a woman joined, then `5`, in UTF-16 units.
        let text = "é👨\u{200d}👩\u{200d}5";
        assert!(!splits_a_character(text, units(0, 1)), "the é");
        assert!(!splits_a_character(text, units(1, 7)), "the joined pair");
        assert!(splits_a_character(text, units(3, 4)), "the first joiner");
        assert!(splits_a_character(text, units(4, 6)), "the woman alone");
        assert!(splits_a_character(text, units(1, 2)), "half the man");
        assert!(!splits_a_character(text, units(7, 8)), "the 5");
        let accented = "e\u{301}x";
        assert!(
            splits_a_character(accented, units(0, 1)),
            "e without its accent"
        );
        assert!(
            !splits_a_character(accented, units(0, 2)),
            "e and its accent"
        );
    }

    /// Every caret place inside a character drawn as one splits it, in
    /// every script (Hangul jamo, Indic conjuncts and vowel signs, a
    /// Prepend mark), and every place between them does not; a mark after
    /// a newline stands alone, and a joiner joins only an emoji to one.
    #[test]
    fn every_place_inside_a_character_drawn_as_one_splits_it() {
        let clusters = [
            "a",
            "e\u{301}",
            "👨\u{200d}👩\u{200d}👧",
            "🇫🇷",
            "🇩🇪",
            "👍🏽",
            "1\u{fe0f}\u{20e3}",
            "\r\n",
            "日",
            "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
            "\u{1112}\u{1161}\u{11ab}",
            "\u{d55c}\u{11ab}",
            "क्षि",
            "\u{0995}\u{09cd}\u{09b7}",
            "\u{0b95}\u{0bc1}",
            "\u{0e01}\u{0e33}",
            "\u{1780}\u{17d2}\u{1780}",
            "\u{0600}1",
            "\n",
            "\u{301}",
            "a\u{200d}",
            "👩",
            "x",
        ];
        let text: String = clusters.concat();
        let mut starts = vec![0];
        for cluster in clusters {
            starts.push(starts.last().copied().unwrap_or(0) + cluster.encode_utf16().count());
        }
        for edge in 0..=text.encode_utf16().count() {
            assert_eq!(
                splits_a_character(&text, units(edge, edge)),
                !starts.contains(&edge),
                "unit {edge} of {text:?}"
            );
        }
    }

    /// A press lands on the character drawn under it: a tab before a
    /// secret placeholder is drawn wider than one cell, and an accent
    /// joined to its letter takes none, so a cell is not a character
    /// column (a press there was taken as inside or outside the
    /// placeholder wrongly).
    #[test]
    fn a_press_lands_on_the_character_drawn_under_it() {
        // `a:`, then a tab drawn up to cell 4, then the placeholder.
        let text = "x: 1\na:\t\u{ab}secret\u{bb}\n";
        let line_start = 5;
        assert_eq!(
            press_units(text, 1, 4),
            line_start + 3,
            "on its opening mark"
        );
        assert_eq!(press_units(text, 1, 5), line_start + 4, "inside it");
        // `e` and its accent are one cell.
        let text = "e\u{301}=\u{ab}s\u{bb}";
        assert_eq!(press_units(text, 0, 2), 3, "on its opening mark");
    }

    /// Input made before a host value was applied is made on that value
    /// instead of being lost (P4): the input's change rebased over the
    /// host's.
    #[test]
    fn input_is_replayed_on_a_document_replaced_meanwhile() {
        let replayed = |before: &str, latest: &str, current: &str| {
            let change = replay(&Change::diffed(before, latest), before, current)?;
            let caret = change.hull().map(|(_, written)| usize::from(written.end()));
            Some((change.apply(current)?, caret?))
        };
        assert_eq!(
            replayed("listen: 1\n", "listen: 1\nQ", "x: 1\n"),
            Some(("x: 1\nQ".to_string(), 6))
        );
        assert_eq!(
            replayed("a: 1\nb: 2\n", "a: 1\nb: 2X\n", "a: 1\nb: 2\nc: 3\n"),
            Some(("a: 1\nb: 2X\nc: 3\n".to_string(), 10))
        );
        assert_eq!(
            replayed("abc", "aXYc", "abc"),
            Some(("aXYc".to_string(), 3))
        );
    }
}
