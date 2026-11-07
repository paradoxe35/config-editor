//! Keys of the documents view.
//!
//! - In the tab list (a WAI-ARIA tab list with automatic activation):
//!   `ArrowLeft` / `ArrowRight` select the previous / next document,
//!   wrapping around, `Home` / `End` the first / last one, `Enter` moves
//!   into the document's editor, `Delete` asks to close the focused one.
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
    documents.select(&next, if from_tabs { Focus::Tab } else { Focus::Editor });
}

/// Arrow keys, `Home`, `End`, `Enter` and `Delete` on a tab.
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
            event.prevent_default();
            documents.request_close(id);
            return;
        }
        "Enter" | " " => {
            event.prevent_default();
            documents.select(id, Focus::Editor);
            return;
        }
        _ => return,
    };
    event.prevent_default();
    if let Some(target) = target {
        documents.select(&target, Focus::Tab);
    }
}
