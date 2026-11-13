//! Keys of the documents view.
//!
//! - In the tab list (a WAI-ARIA tab list with automatic activation):
//!   `ArrowLeft` / `ArrowRight` select the previous / next document,
//!   wrapping around, `Home` / `End` the first / last one, `Enter` (and
//!   `Space`) moves into the document's editor, a typed character moves
//!   there and is typed at its caret, `Delete` asks to close the focused one
//!   when it may be closed (the focus then on the tab taking its place).
//!   The "+" button after the tab list is a plain
//!   button outside it, next in the `Tab` order; no key adds a document
//!   (`Ctrl+T` and `Ctrl+N` are the browser's).
//! - Anywhere in the view, the editor included: `Alt+PageDown` /
//!   `Alt+PageUp` switch to the next / previous document, the focus going
//!   with them. Browsers keep `Ctrl+Tab` and `Ctrl+PageDown` for their own
//!   tabs; `Alt+PageDown` is free in browsers and in the editor's key map,
//!   and needs no `Ctrl+Alt` (which is AltGr on Windows layouts).

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

use super::entries::{Documents, Focus};
use super::plan::step;

/// Whether `event` is the switch shortcut, and which way it goes.
fn switch_offset(event: &KeyboardEvent) -> Option<isize> {
    if !event.alt_key() || event.ctrl_key() || event.meta_key() || event.shift_key() {
        return None;
    }
    match event.key().as_str() {
        "PageDown" => Some(1),
        "PageUp" => Some(-1),
        _ => None,
    }
}

/// `Alt+PageDown` / `Alt+PageUp` on the view's root: the next / previous
/// document, focusing its editor when the key came from an editor, its tab
/// when it came from the tab list.
pub(super) fn on_root_keydown(documents: Documents, event: &KeyboardEvent) {
    let Some(offset) = switch_offset(event) else {
        return;
    };
    event.prevent_default();
    event.stop_propagation();
    let ids = documents.ids();
    let Some(next) = step(&ids, documents.active_id().as_deref(), offset) else {
        return;
    };
    let from_tabs = documents.strip.get_untracked().is_some_and(|strip| {
        event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
            .is_some_and(|target| strip.contains(Some(&target)))
    });
    documents.pick(&next, if from_tabs { Focus::Tab } else { Focus::Editor });
}

/// Whether `key` (a `KeyboardEvent.key` without `Ctrl`, `Alt` or `Meta`)
/// types a character: one character, not a named key (`Tab`, `Dead`, …)
/// and not `Space`, which activates the tab.
fn typed(key: &str) -> bool {
    let mut characters = key.chars();
    characters.next().is_some_and(|first| first != ' ') && characters.next().is_none()
}

/// A character typed on the tab of `id`: the focus moves into its editor
/// now, so the key lands there, at the editor's caret, and so does what
/// is typed after it (a `Space` included). An editor not on screen yet
/// takes the focus once it is; the key itself does nothing then.
fn type_into_editor(documents: Documents, id: &str, event: &KeyboardEvent) {
    let shown = documents.active_id().as_deref() == Some(id)
        && documents
            .entry(id)
            .is_some_and(|entry| untrack(|| entry.handle.is_mounted()))
        && !documents.panel_inert(id);
    match documents.entry(id).filter(|_| shown) {
        Some(entry) => entry.handle.focus(),
        None => {
            event.prevent_default();
            documents.pick(id, Focus::Editor);
        }
    }
}

/// Arrow keys, `Home`, `End`, `Enter`, `Delete` and typed characters on a
/// tab.
pub(super) fn on_tab_keydown(documents: Documents, id: &str, event: &KeyboardEvent) {
    if event.ctrl_key() || event.meta_key() || event.alt_key() {
        return;
    }
    let ids = documents.ids();
    let target = match event.key().as_str() {
        "ArrowRight" => step(&ids, Some(id), 1),
        "ArrowLeft" => step(&ids, Some(id), -1),
        "Home" => ids.first().cloned(),
        "End" => ids.last().cloned(),
        "Delete" => {
            if documents
                .entry(id)
                .is_some_and(|entry| untrack(|| documents.closes(&entry)))
            {
                event.prevent_default();
                documents.request_close(id);
            }
            return;
        }
        "Enter" | " " => {
            event.prevent_default();
            documents.pick(id, Focus::Editor);
            return;
        }
        key if typed(key) => {
            type_into_editor(documents, id, event);
            return;
        }
        _ => return,
    };
    event.prevent_default();
    if let Some(target) = target {
        documents.pick(&target, Focus::Tab);
    }
}
