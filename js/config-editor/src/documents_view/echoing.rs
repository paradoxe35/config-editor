//! A documents view's echoes (the `echoes` option, see `crate::echoes`):
//! a listed text its editor sent and moved past keeps what the editor
//! holds, and a host left on such a text hears the document again.

use std::collections::HashSet;

use config_editor_leptos::Document;
use config_editor_leptos::js_support::flush_pending_input;
use leptos::prelude::{GetValue, UpdateValue, WithValue};
use wasm_bindgen::JsValue;

use super::State;
use crate::documents_list::Listed;
use crate::echoes::{Echo, Echoes};
use crate::mount::call_host_with;
use crate::remap::moved_between;
use crate::settle;

/// The listed `document`'s text is an echo: it keeps `current`, and the
/// ranges listed with it (in the echoed text) move to `current`.
fn keep(document: &mut Document, current: &str) {
    if document.value == current {
        return;
    }
    if !document.read_only_ranges.is_empty() || !document.host_diagnostics.is_empty() {
        let (ranges, diagnostics) = moved_between(
            &document.value,
            current,
            Some(std::mem::take(&mut document.read_only_ranges)),
            Some(std::mem::take(&mut document.host_diagnostics)),
        );
        document.read_only_ranges = ranges.unwrap_or_default();
        document.host_diagnostics = diagnostics.unwrap_or_default();
    }
    current.clone_into(&mut document.value);
}

impl State {
    /// With `echoing`, takes echoes out of `listed`: a document whose
    /// listed text is one its editor sent and has moved past keeps what it
    /// holds (the texts sent from `flushed` on, while the list was read,
    /// were unknown to the host). Returns the documents whose host was left on such an echo,
    /// with the text to send it again, and whether a document waits for
    /// the check after a pause ([`Echo::Settle`]).
    pub(super) fn keep_moved_past(
        &self,
        listed: &mut Listed,
        flushed: u64,
    ) -> (Vec<(String, String)>, bool) {
        let mut resend = Vec::new();
        let mut settles = false;
        if !self.echoing {
            return (resend, settles);
        }
        let _ = self.echoes.try_update_value(|echoes| {
            {
                // The documents no longer listed: one lookup each.
                let ids: HashSet<&str> = listed
                    .documents
                    .iter()
                    .map(|document| document.id.as_str())
                    .collect();
                echoes.retain(|id, _| ids.contains(id.as_str()));
            }
            let now = js_sys::Date::now();
            // Each document's text read where it is kept, not copied.
            self.texts.try_with_value(|texts| {
                for document in &mut listed.documents {
                    let echo = echoes
                        .entry(document.id.clone())
                        .or_insert_with(|| Echoes::new(true));
                    let Some(current) = texts.get(&document.id).map(|texts| &texts.value) else {
                        // Listed for the first time.
                        echo.given(&document.value);
                        continue;
                    };
                    echo.at(now);
                    let echo = echo.listed(&document.value, current, flushed);
                    if echo == Echo::Apply {
                        continue;
                    }
                    settles |= echo != Echo::Skip;
                    keep(document, current);
                    if echo == Echo::Resend {
                        resend.push((document.id.clone(), current.clone()));
                    }
                }
            })
        });
        (resend, settles)
    }

    /// The host gave `value` of its own for the document `id`
    /// (`replaceValue`): what it holds, no echo to wait for.
    pub(super) fn given(&self, id: &str, value: &str) {
        let _ = self.echoes.try_update_value(|echoes| {
            if let Some(echo) = echoes.get_mut(id) {
                echo.replaced(value);
            }
        });
    }

    /// Sends the documents in `resend` to the host's `onChange` again; with
    /// `settles`, checks again once a pause is over.
    pub(super) fn tell_again(self, resend: Vec<(String, String)>, settles: bool) {
        self.tell(resend);
        if settles {
            settle::arm(self.settle, move || self.settle_all());
        }
    }

    /// The pause is over: each document whose host is still on an older
    /// text hears it again.
    fn settle_all(self) {
        flush_pending_input();
        let ids: Vec<String> = self
            .echoes
            .try_with_value(|echoes| echoes.keys().cloned().collect())
            .unwrap_or_default();
        let mut resend = Vec::new();
        for id in ids {
            let Some(texts) = self.texts_of(&id) else {
                continue;
            };
            let again = self.echoes.try_update_value(|echoes| {
                echoes
                    .get_mut(&id)
                    .is_some_and(|echo| echo.settle(&texts.value))
            });
            if again == Some(true) {
                resend.push((id, texts.value));
            }
        }
        self.tell(resend);
    }

    fn tell(self, resend: Vec<(String, String)>) {
        if resend.is_empty() {
            return;
        }
        if let Some(callback) = self.on_change.try_get_value().flatten() {
            for (id, text) in resend {
                call_host_with(
                    &callback,
                    &[JsValue::from_str(&id), JsValue::from_str(&text)],
                );
            }
        }
    }
}
