//! The documents view's side of Find uses: an editor's search reads every
//! other open document (each walked once per request, from its current
//! text, unsaved edits included), and a use chosen in another document
//! shows that document with its caret there, as go to definition does.

use leptos::prelude::*;

use super::entries::{Documents, Focus};
use crate::FieldPath;
use crate::SchemaSource;
use crate::editor::key_names::{DocumentUses, NameRef, OpenUses, SchemaIdentity, uses_in_language};

/// One document's editor's view of the others.
pub(super) struct ViewUses {
    pub(super) documents: Documents,
    /// The asking document's id: its editor reads its own text.
    pub(super) id: String,
}

impl OpenUses for ViewUses {
    fn uses(&self, name: &NameRef) -> Vec<DocumentUses> {
        let entries = self
            .documents
            .entries
            .try_with_untracked(Clone::clone)
            .unwrap_or_default();
        entries
            .iter()
            .filter(|entry| entry.id != self.id)
            .filter_map(|entry| {
                let Some(SchemaSource::Compiled(schema)) = entry.setup.schema.as_ref() else {
                    return None;
                };
                let language = entry.setup.language;
                let hits = entry.value.try_with_untracked(|text| {
                    uses_in_language(
                        text,
                        language,
                        &schema.bundle.schema,
                        &schema.bundle.identity,
                        name,
                    )
                })?;
                (!hits.is_empty()).then(|| DocumentUses {
                    id: entry.id.clone(),
                    label: entry.file_name.try_get_untracked().unwrap_or_default(),
                    hits,
                })
            })
            .collect()
    }

    fn names_map(&self, schema: &SchemaIdentity, pointer: &str) -> bool {
        let asked = NameRef {
            schema: schema.clone(),
            pointer: pointer.to_string(),
            name: String::new(),
        };
        self.documents
            .entries
            .try_with_untracked(|entries| {
                entries
                    .iter()
                    .any(|entry| match entry.setup.schema.as_ref() {
                        Some(SchemaSource::Compiled(compiled)) => {
                            asked.mentioned_in(&compiled.bundle.schema, &compiled.bundle.identity)
                        }
                        _ => false,
                    })
            })
            .unwrap_or(false)
    }

    fn is_open(&self, source: &str) -> bool {
        self.documents.entry(source).is_some()
    }

    fn go_to(&self, id: &str, path: &FieldPath) -> bool {
        let Some(target) = self.documents.entry(id) else {
            return false;
        };
        self.documents.select(&target.id, Focus::Editor);
        self.documents.mount_waiting();
        untrack(|| target.handle.reveal(path))
    }
}
