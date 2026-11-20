//! A documents view's side of each document's channel to the host (see
//! `host_channel`): a listed text its editor sent and moved past keeps
//! what the editor holds, its ranges and diagnostics move to it, and a
//! host left on such a text hears the document again.

use std::collections::HashMap;

use config_editor_leptos::TextRange;
use config_editor_leptos::js_support::{Echo, flush_pending_input};
use leptos::prelude::{GetValue, UpdateValue, WithUntracked, WithValue};
use wasm_bindgen::JsValue;

use super::State;
use crate::documents_list::Listed;
use crate::host_channel::Given;
use crate::mount::call_host_with;
use crate::settle;

impl State {
    /// Takes `listed`: each document listed before goes through its
    /// channel's second step (see `HostChannel::received`), the texts sent
    /// from `flushed` on (while the list was read) unknown to the host. A
    /// document whose listed text keeps what its editor holds is listed
    /// with that text, and with the ranges its editor has when they stay.
    /// Returns the documents whose host was left on an older text, with
    /// the text to send it again, and whether a document waits for the
    /// check after a pause ([`Echo::Settle`]).
    pub(super) fn take_listed(
        &self,
        listed: &mut Listed,
        flushed: u64,
    ) -> (Vec<(String, String)>, bool) {
        let mut resend = Vec::new();
        let mut settles = false;
        let Listed {
            documents, ranges, ..
        } = listed;
        // The ranges each editor has, for the ones that stay.
        let shown: HashMap<String, Vec<TextRange>> = self
            .documents
            .try_with_untracked(|shown| {
                shown
                    .iter()
                    .filter(|document| ranges.contains_key(&document.id))
                    .map(|document| (document.id.clone(), document.read_only_ranges.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let _ = self.kept.try_update_value(|kept| {
            // Each document's text read where it is kept, not copied.
            self.texts.try_with_value(|texts| {
                for document in documents.iter_mut() {
                    let (Some(listed), Some(current), Some(channel)) = (
                        ranges.remove(&document.id),
                        texts.get(&document.id).map(|texts| &texts.value),
                        kept.channels.get_mut(&document.id),
                    ) else {
                        // Listed for the first time: taken as listed.
                        continue;
                    };
                    let diagnostics = std::mem::take(&mut document.host_diagnostics);
                    let taken = channel.received(
                        Given {
                            value: Some(&document.value),
                            revision: None,
                            ranges_for: None,
                            ranges: listed,
                            diagnostics: Some(diagnostics),
                            flushed,
                            stale: false,
                        },
                        current,
                    );
                    document.host_diagnostics = taken.diagnostics.unwrap_or_default();
                    document.read_only_ranges = taken
                        .ranges
                        .or_else(|| shown.get(&document.id).cloned())
                        .unwrap_or_default();
                    document.read_only_ranges_version = channel.version();
                    match taken.echo {
                        None | Some(Echo::Apply) => continue,
                        Some(Echo::Skip) => {}
                        Some(Echo::Settle) => settles = true,
                        Some(Echo::Resend) => {
                            settles = true;
                            resend.push((document.id.clone(), current.clone()));
                        }
                    }
                    current.clone_into(&mut document.value);
                }
            })
        });
        (resend, settles)
    }

    /// The host gave `value` of its own for the document `id`
    /// (`replaceValue`), its editor holding `current`: the ranges to hand
    /// it there and their version, `None` when it is `current` (see
    /// `HostChannel::replaced`).
    pub(super) fn replaced(
        &self,
        id: &str,
        value: &str,
        current: &str,
    ) -> Option<(Vec<TextRange>, u64)> {
        self.kept
            .try_update_value(|kept| {
                let channel = kept.channels.get_mut(id)?;
                let ranges = channel.replaced(value, current)?;
                Some((ranges, channel.version()))
            })
            .flatten()
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
        let resend = self
            .kept
            .try_update_value(|kept| {
                self.texts.try_with_value(|texts| {
                    kept.channels
                        .iter_mut()
                        .filter_map(|(id, channel)| {
                            let current = &texts.get(id)?.value;
                            channel
                                .settle(current)
                                .then(|| (id.clone(), current.clone()))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .flatten()
            .unwrap_or_default();
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
