//! [`DocumentsHandle`]: what a host holds to switch documents, save them and
//! reach their editors.

use leptos::prelude::*;

use super::entries::{Documents, Focus};
use crate::EditorHandle;

/// A handle on a mounted [`ConfigDocuments`](crate::ConfigDocuments): the
/// active document, switching, saving, and each document's
/// [`EditorHandle`].
///
/// Create one, pass it as the `handle` prop, and call it from anywhere (it
/// is `Copy`). Before the view mounts, and after it unmounts, the queries
/// answer `None` (or `false`) and the actions do nothing.
///
/// ```ignore
/// use config_editor_leptos::{ConfigDocuments, DocumentsHandle};
///
/// let tabs = DocumentsHandle::new();
/// view! {
///     <button on:click=move |_| { tabs.save_all(); }>"Save all"</button>
///     <button on:click=move |_| { tabs.select("tls"); }>"Show tls.toml"</button>
///     <ConfigDocuments handle=tabs documents=documents />
/// }
/// ```
#[derive(Clone, Copy)]
pub struct DocumentsHandle {
    documents: RwSignal<Option<Documents>, LocalStorage>,
}

impl Default for DocumentsHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DocumentsHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DocumentsHandle")
            .field("mounted", &self.is_mounted())
            .finish()
    }
}

impl DocumentsHandle {
    /// A handle not attached to a view yet.
    pub fn new() -> Self {
        Self {
            documents: RwSignal::new_local(None),
        }
    }

    /// Attaches the view that mounted with this handle, until it unmounts.
    pub(super) fn attach(self, documents: Documents) {
        self.documents.set(Some(documents));
        on_cleanup(move || {
            let _ = self.documents.try_set(None);
        });
    }

    fn view(self) -> Option<Documents> {
        self.documents.try_get_untracked().flatten()
    }

    /// Takes the host's new list now, as the view does once its effects
    /// run: the active document (and every query) follows it at once.
    pub(crate) fn take_documents(&self, documents: &[super::Document]) {
        if let Some(view) = self.view() {
            untrack(|| view.sync(documents));
        }
    }

    /// Whether a view is mounted with this handle. Reactive.
    pub fn is_mounted(&self) -> bool {
        self.documents.try_with(Option::is_some).unwrap_or(false)
    }

    /// The active document's id. Reactive.
    pub fn active(&self) -> Option<String> {
        self.documents
            .try_get()
            .flatten()
            .and_then(|documents| documents.active.try_get().flatten())
    }

    /// Shows the document `id` and focuses its editor (its caret where it
    /// was), as selecting its tab and pressing `Enter` does. Returns whether
    /// the document is open.
    pub fn select(&self, id: &str) -> bool {
        self.view()
            .is_some_and(|documents| documents.select(id, Focus::Editor))
    }

    /// Shows the document `id`, leaving the focus where it is (a host
    /// following its own state of which file is open). Returns whether the
    /// document is open.
    pub fn show(&self, id: &str) -> bool {
        self.view()
            .is_some_and(|documents| documents.select(id, Focus::Keep))
    }

    /// The editor of the document `id`, for paths, folds, saving and the
    /// rest of [`EditorHandle`]. It is attached once the document has been
    /// shown: editors mount the first time their document is selected.
    pub fn editor(&self, id: &str) -> Option<EditorHandle> {
        Some(self.view()?.entry(id)?.handle)
    }

    /// The active document's editor.
    pub fn active_editor(&self) -> Option<EditorHandle> {
        let documents = self.view()?;
        Some(documents.entry(&documents.active_id()?)?.handle)
    }

    /// Whether the document `id` has unsaved edits (its text differs from
    /// its baseline). Reactive.
    pub fn is_dirty(&self, id: &str) -> bool {
        self.documents
            .try_get()
            .flatten()
            .and_then(|documents| {
                documents
                    .entries
                    .try_with(|entries| entries.iter().find(|entry| entry.id == id).cloned())
                    .flatten()
            })
            .is_some_and(|entry| entry.dirty.try_get().unwrap_or(false))
    }

    /// Saves the document `id` as `Ctrl+S` in its editor does, through the
    /// `on_save` prop. Returns whether it was sent (see
    /// [`EditorHandle::save`]).
    pub fn save(&self, id: &str) -> bool {
        self.view().is_some_and(|documents| documents.save(id))
    }

    /// Saves every document with unsaved edits, in order, through
    /// `on_save`. Returns the ids sent: a document whose `edit_as` edit
    /// cannot convert back yet is left out.
    pub fn save_all(&self) -> Vec<String> {
        self.view()
            .map(|documents| documents.save_all())
            .unwrap_or_default()
    }

    /// Focuses the active document's editor, keeping its selection.
    pub fn focus(&self) {
        if let Some(documents) = self.view()
            && let Some(id) = documents.active_id()
        {
            documents.select(&id, Focus::Editor);
        }
    }
}
