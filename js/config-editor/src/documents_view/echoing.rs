//! A documents view's side of each document's record of its host (the
//! core's `host_inputs`): every listed document's update goes through its
//! editor's one entry when it is mounted (which tells an echo from the
//! host's own text, places the lists where they name places, and has a
//! host left on an older text hear the document again), else through the
//! record alone. A listed text that keeps what the editor holds is listed
//! with that text.

use leptos::prelude::{UpdateValue, WithValue};
use leptos_config_editor::js_support::{self, HostChannel, HostUpdate, Taken};
use wasm_bindgen::prelude::*;

use super::State;
use crate::documents_list::Listed;

impl State {
    /// The record of the document `id`'s host.
    pub(super) fn channel_of(&self, id: &str) -> Option<HostChannel> {
        self.kept
            .try_with_value(|kept| kept.channels.get(id).cloned())
            .flatten()
    }

    /// Hands `update` to the document `id`'s editor when it is mounted,
    /// else to its record alone, the host's text being `current`.
    pub(super) fn take(
        &self,
        id: &str,
        channel: &HostChannel,
        update: HostUpdate,
        current: &str,
    ) -> Result<Taken, JsError> {
        let mounted = self
            .handle
            .editor(id)
            .filter(leptos_config_editor::EditorHandle::is_mounted);
        let taken =
            match mounted.and_then(|editor| js_support::host_update(&editor, update.clone())) {
                Some(taken) => taken,
                None => channel.take_unmounted(update, current),
            }
            .map_err(|error| JsError::new(&error.to_string()))?;
        for warning in &taken.warnings {
            web_sys::console::warn_1(&format!("config-editor: {id}: {warning}").into());
        }
        Ok(taken)
    }

    /// Takes `listed`: the new records are bound to the view (their editors
    /// take them over), and each document listed before gives its update.
    /// A document whose listed text keeps what its editor holds is listed
    /// with that text. Returns the errors of the documents whose update
    /// was refused (their lists name a revision never sent).
    pub(super) fn take_listed(&self, listed: &mut Listed) -> Vec<JsError> {
        for (id, channel) in listed.fresh.drain(..) {
            js_support::bind_document_channel(&self.handle, &id, channel);
        }
        let mut refused = Vec::new();
        let Listed {
            documents, updates, ..
        } = listed;
        for document in documents.iter_mut() {
            let (Some(update), Some(channel), Some(current)) = (
                updates.remove(&document.id),
                self.channel_of(&document.id),
                self.texts_of(&document.id).map(|texts| texts.value),
            ) else {
                continue;
            };
            let shown = match self.take(&document.id, &channel, update, &current) {
                Ok(taken) => taken.shown,
                Err(error) => {
                    refused.push(error);
                    false
                }
            };
            if !shown {
                current.clone_into(&mut document.value);
            }
        }
        refused
    }

    /// The host gave `value` of its own for the document `id`
    /// (`replaceValue`), its editor holding `current`: whether it replaced
    /// the text.
    pub(super) fn replaced(&self, id: &str, value: &str, current: &str) -> bool {
        let Some(channel) = self.channel_of(id) else {
            return false;
        };
        let update = HostUpdate {
            value: Some(value.to_string()),
            reset: true,
            ..HostUpdate::default()
        };
        self.take(id, &channel, update, current)
            .is_ok_and(|taken| taken.shown)
    }

    /// The document `id`'s last text sent reached no host (no `onChange`).
    pub(super) fn untold(&self, id: &str) {
        let _ = self.kept.try_update_value(|kept| {
            if let Some(channel) = kept.channels.get(id) {
                channel.untold();
            }
        });
    }
}
