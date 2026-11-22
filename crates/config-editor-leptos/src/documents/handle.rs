//! [`DocumentsHandle`]: what a host holds to switch documents, save them and
//! reach their editors.

use leptos::prelude::*;

use super::entries::{Documents, Focus};
use crate::{EditorHandle, Language};

/// A handle on a mounted [`ConfigDocuments`](crate::ConfigDocuments): the
/// active document, switching, saving, and each document's
/// [`EditorHandle`].
///
/// Create one, pass it as the `handle` prop, and call it from anywhere (it
/// is `Copy`). Before the view mounts, and after it unmounts, the queries
/// answer `None` (or `false`) and the actions do nothing.
///
/// ```ignore
/// use leptos_config_editor::{ConfigDocuments, DocumentsHandle};
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
    /// The documents' host records a binding made before the view mounted
    /// (see [`DocumentsHandle::bind_channel`]), handed to it as it does.
    channels:
        StoredValue<std::collections::HashMap<String, crate::editor::HostChannel>, LocalStorage>,
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
            channels: StoredValue::new_local(std::collections::HashMap::new()),
        }
    }

    /// Attaches the view that mounted with this handle, until it unmounts.
    pub(super) fn attach(self, documents: Documents) {
        let channels = self
            .channels
            .try_update_value(std::mem::take)
            .unwrap_or_default();
        for (id, channel) in channels {
            documents.bind_channel(&id, channel);
        }
        self.documents.set(Some(documents));
        on_cleanup(move || {
            let _ = self.documents.try_set(None);
        });
    }

    fn view(self) -> Option<Documents> {
        self.documents.try_get_untracked().flatten()
    }

    /// The view, its waiting editors mounted (see `Documents::mount_waiting`):
    /// for the queries that read a document's editor.
    fn mounted_view(self) -> Option<Documents> {
        let documents = self.view()?;
        documents.mount_waiting();
        Some(documents)
    }

    /// Takes the host's new list now, as the view does once its effects
    /// run: the active document (and every query) follows it at once.
    pub(crate) fn take_documents(&self, documents: &[super::Document]) {
        if let Some(view) = self.view() {
            untrack(|| view.sync(documents));
        }
    }

    /// The language the document `id` is edited as, as its list or
    /// [`DocumentsHandle::edit_as`] asked last; `None` when it is not open.
    pub(crate) fn edit_as_of(&self, id: &str) -> Option<Option<crate::Language>> {
        let entry = self.view()?.entry(id)?;
        Some(entry.edit_as.get_untracked())
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
    /// shown: editors mount the first time their document is selected, or
    /// at once for a document listed with an
    /// [`edit_as_text`](crate::Document::edit_as_text).
    pub fn editor(&self, id: &str) -> Option<EditorHandle> {
        Some(self.mounted_view()?.entry(id)?.handle)
    }

    /// The revision of the text `on_change` sent last for the document
    /// `id`, while the host has not handed it back (with `echoes`): pass it
    /// back as the document's `value_revision` with that text, as
    /// [`EditorHandle::sent_revision`] says.
    pub fn sent_revision(&self, id: &str) -> Option<u64> {
        self.editor(id)?.sent_revision()
    }

    /// The document `id`'s host keeps its record in `channel` (the
    /// JavaScript `ConfigDocuments`): its editor takes it over when it
    /// mounts, and the binding hands it every update.
    pub(crate) fn bind_channel(&self, id: &str, channel: crate::editor::HostChannel) {
        match self.view() {
            Some(documents) => documents.bind_channel(id, channel),
            None => {
                let _ = self.channels.try_update_value(|channels| {
                    channels.insert(id.to_string(), channel);
                });
            }
        }
    }

    /// The active document's editor.
    pub fn active_editor(&self) -> Option<EditorHandle> {
        let documents = self.mounted_view()?;
        Some(documents.entry(&documents.active_id()?)?.handle)
    }

    /// Whether the document `id` has unsaved edits (its text differs from
    /// its baseline). Reactive.
    pub fn is_dirty(&self, id: &str) -> bool {
        let _ = self.mounted_view();
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
        self.mounted_view()
            .is_some_and(|documents| documents.save(id))
    }

    /// Saves every document with unsaved edits, in order, through
    /// `on_save`. Returns the ids sent: a document whose text does not
    /// read (shown or not), or whose `edit_as` edit cannot convert back
    /// yet, is left out.
    pub fn save_all(&self) -> Vec<String> {
        self.mounted_view()
            .map(|documents| documents.save_all())
            .unwrap_or_default()
    }

    /// Edits the document `id` as `language` (its own language to go back),
    /// as a new [`Document::edit_as`](crate::Document::edit_as) does, and
    /// also when `language` is what `edit_as` already says: a list with an
    /// unchanged `edit_as` changes nothing, so after an undo stepped back
    /// across the switch this is how to ask for it again. A switch the text
    /// cannot make yet waits, as one from the list does; asking for the
    /// language on screen ends one waiting. Give your list's document the
    /// same `edit_as`, or the next list switches back to it. Returns
    /// whether the document is open.
    pub fn edit_as(&self, id: &str, language: Language) -> bool {
        self.view()
            .is_some_and(|documents| documents.edit_as(id, language))
    }

    /// Goes to the definition of the name at the active editor's caret, as
    /// `F12` does there (see [`EditorHandle::go_to_definition`]). Returns
    /// whether there was a known name at the caret.
    pub fn go_to_definition(&self) -> bool {
        self.active_editor()
            .is_some_and(|editor| editor.go_to_definition())
    }

    /// Lists the uses of the name at the active editor's caret, as
    /// `Shift+F12` does there (see [`EditorHandle::find_uses`]). Returns
    /// whether there was a name at the caret.
    pub fn find_uses(&self) -> bool {
        self.active_editor()
            .is_some_and(|editor| editor.find_uses())
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
