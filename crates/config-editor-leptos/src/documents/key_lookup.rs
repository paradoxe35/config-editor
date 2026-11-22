//! The documents view's side of key-name suggestions: it keeps the names
//! index (see `key_names`) in step with its list and its documents' texts,
//! answers each editor's lookup from the other open documents and the
//! host's names, and takes Go to definition to the open document defining
//! a name.

use leptos::prelude::*;

use super::entries::{Documents, Entry, Focus};
use super::key_names::{Listed, identity_of};
use crate::editor::key_names::{KeyNameSource, KeyNamesInput, SchemaIdentity};
use crate::{FieldPath, GoToDefinition, KeyName, KeyNames};

impl Documents {
    /// The index takes the view's new list: a closed document's names go.
    pub(super) fn list_names(&self, entries: &[Entry]) {
        let listed = entries
            .iter()
            .map(|entry| Listed {
                id: &entry.id,
                file_name: entry.file_name.try_get_untracked().unwrap_or_default(),
                identity: identity_of(entry.setup.schema.as_ref()),
                language: entry.setup.language,
            })
            .collect();
        self.names.update_value(|names| names.list(listed));
    }

    /// The text of document `id` changed: only its names are read again.
    pub(super) fn text_changed(&self, id: &str) {
        let _ = self.names.try_update_value(|names| names.edited(id));
    }

    /// The host's names for every document (the view's `key_names`).
    pub(super) fn give_key_names(&self, names: Signal<Vec<KeyNames>>) {
        self.host_names.set_value(names);
    }

    /// The names at `pointer` of the documents of `schema`, for a field of
    /// document `id`: every other open document's, in tab order, then the
    /// host's (the view's and the document's own), each once. The host's
    /// lists are read tracked: a new list runs the asking suggestions again.
    pub(super) fn key_names_for(
        &self,
        id: &str,
        schema: &SchemaIdentity,
        pointer: &str,
    ) -> Vec<KeyName> {
        let view = self
            .host_names
            .try_get_value()
            .and_then(|names| names.try_get())
            .unwrap_or_default();
        let own = self
            .entry(id)
            .and_then(|entry| entry.key_names.try_get())
            .unwrap_or_default();
        let mut parse = |id: &str, language: crate::Language| {
            let entry = self.entry(id)?;
            entry
                .value
                .try_with_untracked(|text| crate::editor::kept_or_aside(language, text))
        };
        self.names
            .try_update_value(|names| {
                names.names(schema, pointer, Some(id), &[&view, &own], &mut parse)
            })
            .unwrap_or_default()
    }

    /// What the editor of `entry` reads its names from, and where its Go
    /// to definition goes: to another open document when it defines the
    /// name, else to the host's `on_go_to_definition`.
    pub(super) fn key_names_input(&self, entry: &Entry) -> KeyNamesInput {
        let documents = *self;
        let from = entry.id.clone();
        let host = self.callbacks.on_go_to_definition;
        let id = from.clone();
        let go = Callback::new(move |target: GoToDefinition| {
            let shown = documents.go_to_key(
                &from,
                target.source.as_deref(),
                &target.pointer,
                &target.name,
            );
            if !shown && let Some(host) = host {
                host.run(target);
            }
        });
        KeyNamesInput {
            source: std::sync::Arc::new(ViewNames { documents, id }),
            on_go_to_definition: Some(go),
        }
    }

    /// Shows the open document (not `from`) that `source` stands for (see
    /// `NamesIndex::defining`) with its caret on the key `name` of the map
    /// at `pointer`. Returns whether there is one.
    fn go_to_key(&self, from: &str, source: Option<&str>, pointer: &str, name: &str) -> bool {
        let defining = source.and_then(|source| {
            self.names
                .try_with_value(|names| names.defining(source, pointer, name))
                .flatten()
        });
        let Some(target) = defining
            .filter(|id| id != from)
            .and_then(|id| self.entry(&id))
        else {
            return false;
        };
        self.select(&target.id, Focus::Editor);
        self.mount_waiting();
        let path = FieldPath::from_pointer(pointer).key(name);
        untrack(|| target.handle.reveal(path));
        true
    }
}

/// One document's editor's names: the view's, but its own.
struct ViewNames {
    documents: Documents,
    id: String,
}

impl KeyNameSource for ViewNames {
    fn names(&self, schema: &SchemaIdentity, pointer: &str) -> Vec<KeyName> {
        self.documents.key_names_for(&self.id, schema, pointer)
    }
}
