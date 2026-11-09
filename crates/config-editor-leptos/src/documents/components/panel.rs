//! One document's tab panel and its editor. The editor mounts the first time
//! the document is shown (at once, so its handle answers right after a
//! `select`), then stays mounted while the document is open:
//! hidden panels keep their layout (`visibility: hidden`, `inert`), so the
//! editor's scroll position, caret, undo history, folds and find widget are
//! all there when the document comes back.

use leptos::prelude::*;

use super::super::entries::{Documents, Entry};
use crate::reactive::{later, mount_detached};
use crate::{ConfigEditor, EditorSettings, Language, SaveRequest};

#[component]
pub(in crate::documents) fn DocumentPanel(documents: Documents, entry: Entry) -> impl IntoView {
    let id = StoredValue::new(entry.id.clone());
    let active = documents.follows.active.read();
    let shown = Memo::new(move |_| {
        active.with(|active| id.with_value(|id| active.as_deref() == Some(id.as_str())))
    });
    focus_when_mounted(documents, &entry);
    let panel = mount_when_shown(documents, &entry);
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
            node_ref=panel
        ></div>
    }
}

/// Mounts the document's editor into its panel the first time it is shown:
/// at once when the view shows it (`Entry::mount_now`), or once the panel
/// is on the page for a document shown before. As a view of its own, with
/// an owner of its own, unmounted (then cleaned up) with the panel.
fn mount_when_shown(documents: Documents, entry: &Entry) -> NodeRef<leptos::html::Div> {
    let panel: NodeRef<leptos::html::Div> = NodeRef::new();
    let mounted: StoredValue<Option<Box<dyn std::any::Any>>, LocalStorage> =
        StoredValue::new_local(None);
    let mount = {
        let entry = entry.clone();
        move |()| {
            let built = mounted.try_with_value(Option::is_some).unwrap_or(true);
            if built || !entry.visited.get_untracked() {
                return;
            }
            let Some(element) = panel.get_untracked() else {
                return;
            };
            let shown = entry.clone();
            let view = untrack(|| mount_detached(element.into(), move || editor(documents, shown)));
            mounted.set_value(Some(view));
            report_language_once_mounted(documents, entry.clone());
        }
    };
    let mount = Callback::new(mount);
    entry.mount_now.set_value(Some(mount));
    let visited = entry.visited;
    Effect::new(move |_| {
        if panel.get().is_some() && visited.get() {
            mount.run(());
        }
    });
    let mount_now = entry.mount_now;
    on_cleanup(move || {
        let _ = mount_now.try_set_value(None);
        let _ = mounted.try_update_value(|view| *view = None);
    });
    panel
}

/// Tells the host the language on screen of the editor just mounted, a
/// moment later, so its callback can already reach the editor (see
/// `Documents::note_language`: the editor's own report of an `edit_as`
/// it mounted with is not repeated).
fn report_language_once_mounted(documents: Documents, entry: Entry) {
    later::soon(documents, move |documents| {
        if let Some(language) = entry.handle.language_on_screen() {
            documents.note_language(&entry, language);
        }
    });
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
        later::next_frame(documents, move |_| handle.focus());
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
    let language_entry = entry.clone();
    let language_changed =
        move |language: Language| documents.note_language(&language_entry, language);
    let sync_entry = entry.clone();
    let sync_changed = move |in_sync: bool| documents.note_sync(&sync_entry, in_sync);
    // A text kept from an earlier session opens in the `edit_as` language.
    let opening = entry
        .edit_as_text
        .try_update_value(std::mem::take)
        .flatten();
    entry
        .handle
        .open_with(opening.filter(|_| entry.edit_as.get_untracked().is_some()));
    let read_only_reason = entry.read_only_reason;
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
            read_only_reason=read_only_reason
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
            on_edit_as_change=language_changed
            on_sync_change=sync_changed
            handle=entry.handle
        />
    }
}
