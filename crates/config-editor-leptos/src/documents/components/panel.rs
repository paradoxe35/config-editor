//! One document's tab panel and its editor. The editor mounts the first time
//! the document is shown, then stays mounted while the document is open:
//! hidden panels keep their layout (`visibility: hidden`, `inert`), so the
//! editor's scroll position, caret, undo history, folds and find widget are
//! all there when the document comes back.

use leptos::prelude::*;

use super::super::entries::{Documents, Entry};
use crate::{ConfigEditor, EditorSettings, Language, SaveRequest};

#[component]
pub(in crate::documents) fn DocumentPanel(documents: Documents, entry: Entry) -> impl IntoView {
    let id = StoredValue::new(entry.id.clone());
    let active = documents.follows.active.read();
    let shown = Memo::new(move |_| {
        active.with(|active| id.with_value(|id| active.as_deref() == Some(id.as_str())))
    });
    focus_when_mounted(documents, &entry);
    let visited = entry.visited;
    let editor = {
        let entry = entry.clone();
        move || visited.get().then(|| editor(documents, entry.clone()))
    };
    let class = move || {
        if shown.get() {
            "config-editor-documents__panel"
        } else {
            "config-editor-documents__panel config-editor-documents__panel-hidden"
        }
    };

    view! {
        <div
            role="tabpanel"
            id=entry.panel_id(documents)
            class=class
            aria-labelledby=entry.tab_id(documents)
            inert=move || !shown.get()
            data-document-id=entry.id.clone()
        >
            {editor}
        </div>
    }
}

/// Focuses the document's editor once it is mounted and shown, when a
/// switch asked for it.
fn focus_when_mounted(documents: Documents, entry: &Entry) {
    let id = entry.id.clone();
    let handle = entry.handle;
    let focus_editor = documents.follows.focus_editor.read();
    Effect::new(move |_| {
        let wanted = focus_editor.with(|wanted| wanted.as_deref() == Some(id.as_str()));
        if !wanted || !handle.is_mounted() {
            return;
        }
        documents.focus_editor.set(None);
        // After the panel has lost `inert`.
        request_animation_frame(move || handle.focus());
    });
}

/// The document's editor, its callbacks reporting to the host with the
/// document's id.
fn editor(documents: Documents, entry: Entry) -> impl IntoView {
    let Documents {
        callbacks, follows, ..
    } = documents;
    let id = StoredValue::new(entry.id.clone());
    let value = entry.value;
    let changed = move |text: String| {
        if value.with_untracked(|current| *current != text) {
            value.set(text.clone());
        }
        if let Some(callback) = callbacks.on_change {
            callback.run((id.get_value(), text));
        }
    };
    let saved = callbacks.on_save.map(|callback| {
        Callback::new(move |request: SaveRequest| callback.run((id.get_value(), request)))
    });
    let dirty_entry = entry.clone();
    let dirty_changed = move |dirty: bool| documents.note_dirty(&dirty_entry, dirty);
    let edit_as = entry.edit_as;
    let edit_as = Signal::derive(move || edit_as.get());
    let schema = entry.setup.schema.clone();
    // The view injects the stylesheet once for all its editors.
    let own_settings = entry.settings;
    let view_settings = follows.settings.read();
    let settings = Signal::derive(move || EditorSettings {
        styled: false,
        ..own_settings.get().unwrap_or_else(|| view_settings.get())
    });
    let language: Language = entry.setup.language;

    view! {
        <ConfigEditor
            language=language
            edit_as=edit_as
            value=entry.value
            baseline=entry.baseline
            read_only=entry.read_only
            nostrip:schema=schema
            file_name=entry.file_name.get_untracked()
            settings=settings
            host_diagnostics=entry.host_diagnostics
            read_only_paths=entry.read_only_paths
            read_only_ranges=entry.read_only_ranges
            secrets=entry.secrets.clone()
            on_change=changed
            nostrip:on_save=saved
            on_dirty_change=dirty_changed
            handle=entry.handle
        />
    }
}
