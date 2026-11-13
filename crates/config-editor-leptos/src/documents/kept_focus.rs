//! The focus stays in the view when the control that has it goes: a tab
//! closed or moved by the host, the last tab closed. It goes to the tab of
//! the document shown, and with none open to the note that says so (see
//! `focus::keep_focus`); a tab that only moves gets it back.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::entries::Documents;
use crate::focus::keep_focus;

/// Keeps the focus in the view while it lives.
pub(super) fn keep_focus_in_view(documents: Documents) {
    let root = documents.root;
    // A new root (or none) ends the watch on the old one.
    Effect::new(move |_| {
        if let Some(root) = root.get() {
            untrack(|| keep_focus(&root, move |_| successor(documents)));
        }
    });
}

/// The tab of the document shown, else the note of an empty view.
fn successor(documents: Documents) -> Option<web_sys::HtmlElement> {
    documents
        .active_id()
        .and_then(|id| documents.tab_element(&id))
        .or_else(|| {
            documents
                .empty
                .get_untracked()
                .and_then(|note| note.dyn_into().ok())
        })
}
