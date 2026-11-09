//! The "+" (`on_add`): the document the host adds in answer is selected and
//! its editor focused. Only in answer: the request expires
//! [`ANSWER_MS`] after the click, when the focus moves to something outside
//! the view, when the user picks a document, or clicks or types anywhere
//! in the view but on the "+"; a document the host adds
//! after that (on its own) is listed without being selected. An
//! update that adds nothing (an edit coming back) leaves it waiting.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::entries::Documents;
use crate::reactive::later::{self, Timeout};

/// How long the host has to answer the "+" with a document.
pub(super) const ANSWER_MS: i32 = 5_000;

/// A "+" waiting for the host's document: its expiry timer while it waits.
#[derive(Clone, Copy)]
pub(super) struct Adding(StoredValue<Option<Timeout>>);

impl Adding {
    pub(super) fn new() -> Self {
        Self(StoredValue::new(None))
    }

    /// The user asked for a new document: the next one added within
    /// [`ANSWER_MS`] is selected.
    pub(super) fn start(self, documents: Documents) {
        self.stop();
        let _ = self.0.try_set_value(Some(Timeout::default()));
        let timeout = later::after(documents, ANSWER_MS, |documents| documents.adding.stop());
        // Unless it expired at once (a native test that does not hold timers).
        let _ = self.0.try_update_value(|waiting| {
            if waiting.is_some() {
                *waiting = Some(timeout);
            }
        });
    }

    /// No document added from now on is selected for the "+".
    pub(super) fn stop(self) {
        let _ = self.take();
    }

    /// Whether a "+" was waiting; it no longer is.
    pub(super) fn take(self) -> bool {
        match self.0.try_update_value(Option::take).flatten() {
            Some(timeout) => {
                timeout.cancel();
                true
            }
            None => false,
        }
    }
}

/// A press or a key anywhere in the view but on the "+" itself (a click or
/// typing in a document's editor, a tab, a close button): the user went on
/// with something else, and a waiting "+" expires.
pub(super) fn on_root_input(documents: Documents, event: &web_sys::Event) {
    let on_add = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|element| {
            element
                .closest(".config-editor-documents__add")
                .ok()
                .flatten()
        })
        .is_some();
    if !on_add {
        documents.adding.stop();
    }
}

/// The focus leaving the view's root for something outside it (a field of
/// the host's page): a waiting "+" expires. Leaving for nothing (the window
/// losing the focus, a `prompt` the host opens) keeps it.
pub(super) fn on_root_focusout(documents: Documents, event: &web_sys::FocusEvent) {
    let Some(next) = event
        .related_target()
        .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
    else {
        return;
    };
    if documents
        .root
        .get_untracked()
        .is_some_and(|root| !root.contains(Some(&next)))
    {
        documents.adding.stop();
    }
}
