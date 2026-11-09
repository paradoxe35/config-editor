//! The [`ConfigDocuments`] component: several documents in one view, as
//! tabs. Its parts live in submodules:
//!
//! - `model`: the [`Document`]s the host lists, and the [`CloseRequest`]
//!   the view sends back.
//! - `plan`: the decisions apart from the DOM (a new list against the open
//!   editors, the active document, shared schemas), unit-tested.
//! - `entries`: the open documents as the view holds them, and the shared
//!   context every part acts through.
//! - `keys`, `handle`, `components`: keyboard, the host's
//!   [`DocumentsHandle`], and the view.

use leptos::prelude::*;

mod adding;
mod components;
mod entries;
mod handle;
mod keys;
mod model;
mod plan;
#[cfg(all(test, feature = "yaml", feature = "toml"))]
mod screen_tests;
mod tab_names;

pub use handle::DocumentsHandle;
pub use model::{CloseRequest, Document};

use crate::cssom::cssom;
use crate::reactive::KeyedList;
use crate::{EditorSettings, Language, PRESET_CSS, SaveRequest};
use components::{DocumentPanel, TabStrip};
use entries::{Callbacks, Documents, Entry};

/// Several documents in one view: a tab strip over one editor per document,
/// for hosts with several files (a main config and its drop-ins).
///
/// Each [`Document`] has its own language, text, baseline, read-only state,
/// schema, protected fields, host problems and, if it needs them, its own
/// settings; documents with the same schema share one compiled
/// [`Schema`](crate::Schema). The host owns the list: it adds, removes,
/// renames and reorders documents by changing `documents`, and keeps each
/// `value` up to date from `on_change`. The view reports what the user asks
/// for: `on_select` when they switch, `on_close` when they close one (which
/// only the host does, asking first when it has unsaved edits; a document
/// that is not [`closable`](Document::closable) cannot be closed), `on_add`
/// when they want a new one (which the host creates and lists).
///
/// An editor mounts the first time its document is shown and then stays,
/// hidden while another document is active, so each document keeps its
/// caret, selection, scroll position, undo history, folds and `edit_as`
/// language across switches.
///
/// Keys: `Alt+PageDown` / `Alt+PageUp` switch to the next / previous
/// document from anywhere in the view; in the tab list, arrows, `Home` and
/// `End` move between tabs, `Enter` moves into the editor, `Delete` asks to
/// close a closable one. The "+" button (with `on_add`) comes after the tab
/// list in the `Tab` order. `Ctrl+S` / `Cmd+S` saves the document being
/// edited.
///
/// ```ignore
/// use config_editor_leptos::{ConfigDocuments, Document};
///
/// let documents = RwSignal::new(vec![
///     Document::new("main", "burrow.yaml", main_text).schema(SCHEMA),
///     Document::new("tls", "conf.d/tls.toml", tls_text).schema(SCHEMA),
/// ]);
/// let set_value = move |(id, text): (String, String)| {
///     documents.update(|list| {
///         if let Some(document) = list.iter_mut().find(|document| document.id == id) {
///             document.value = text;
///         }
///     })
/// };
/// view! {
///     <ConfigDocuments
///         documents=documents
///         on_change=set_value
///         on_save=move |(id, request): (String, SaveRequest)| upload(id, request.text)
///         on_close=move |request: CloseRequest| close(request)
///     />
/// }
/// ```
#[component]
pub fn ConfigDocuments(
    /// The open documents, in tab order. A new list updates the view: a
    /// document keeps its editor while its id stays listed (text, baseline,
    /// name and read-only state update in place; a new language or schema
    /// mounts a new editor).
    #[prop(into)]
    documents: Signal<Vec<Document>>,
    /// The document shown first. Unset (or not listed), the first one.
    #[prop(optional, into)]
    initial_active: Option<String>,
    /// Called with a document's id when it becomes the active one: the user
    /// picked its tab or pressed a switch key, the handle selected it, or
    /// the active document closed and this one took its place.
    #[prop(optional, into)]
    on_select: Option<Callback<String>>,
    /// Called when the user asks to close a document (its close button, a
    /// middle click, `Delete` on its tab). Nothing closes by itself: remove
    /// the document from `documents`, asking the user first when
    /// [`CloseRequest::dirty`]. Only [`closable`](Document::closable)
    /// documents ask; without it, no tab has a close button.
    #[prop(optional, into)]
    on_close: Option<Callback<CloseRequest>>,
    /// Called when the user asks for a new document: the "+" button after
    /// the last tab, shown only when this is set. It only asks: the host
    /// decides what a new document is and adds it to `documents`; the one
    /// it adds in answer is selected and its editor focused. In answer: the
    /// first added within 5 seconds of the click, before the focus moves
    /// out of the view (into a dialog of the host's: select the document
    /// with [`DocumentsHandle::select`]), before another tab is picked and
    /// before the user clicks or types anywhere else in the view; one added
    /// later is listed, not selected. The button stays at
    /// the strip's end when the tabs overflow. No key adds one (`Ctrl+T`
    /// and `Ctrl+N` are the browser's). Leave it unset for a view that only
    /// shows documents.
    #[prop(optional, into)]
    on_add: Option<Callback<()>>,
    /// Called with a document's id and its text, in its language, after
    /// every edit. Store the text as the document's `value`.
    #[prop(optional, into)]
    on_change: Option<Callback<(String, String)>>,
    /// Called with a document's id and what to save when the user saves it
    /// (`Ctrl+S` / `Cmd+S` in its editor) or the host calls
    /// [`DocumentsHandle::save`] or [`DocumentsHandle::save_all`]. Without
    /// it, `Ctrl+S` is left to the browser.
    #[prop(optional, into)]
    on_save: Option<Callback<(String, SaveRequest)>>,
    /// Called with a document's id and whether its text differs from its
    /// baseline: once when it is added, then each time that changes.
    #[prop(optional, into)]
    on_dirty_change: Option<Callback<(String, bool)>>,
    /// Called with a document's id and the language on screen in its
    /// editor: once when the editor mounts (the first time the document is
    /// shown), then each time that changes: an `edit_as` switch made, one
    /// that waited until the text could convert, undo or redo stepping back
    /// across a switch. A switch still waiting changes nothing on screen, so
    /// it is not reported; neither is a list that changes nothing.
    #[prop(optional, into)]
    on_edit_as_change: Option<Callback<(String, Language)>>,
    /// Called with a document's id and whether the text on screen is
    /// written back to its `value`: once when its editor mounts, then each
    /// time that changes (see `ConfigEditor`'s `on_sync_change`). `false`
    /// while an `edit_as` edit cannot convert back, from whatever brought
    /// it there (typing, paste, cut, drop, undo, a switch, a reload, a new
    /// `value`, a [`Document::edit_as_text`] that does not convert).
    #[prop(optional, into)]
    on_sync_change: Option<Callback<(String, bool)>>,
    /// The settings of every editor, and the theme and colors of the tabs
    /// (the `Tab*` tokens of [`ThemeColor`](crate::ThemeColor) included); a
    /// document's own [`Document::settings`] replace them for its editor.
    /// May be a signal: every setting switches live. `styled` injects
    /// [`PRESET_CSS`] once, for the whole view.
    #[prop(optional, into)]
    settings: Signal<EditorSettings>,
    /// Extra class on the view's root for host styles.
    #[prop(optional, into)]
    class: Option<String>,
    /// The tab list's accessible name; unset, the `tabList` text of the
    /// `strings` setting ("Open files").
    #[prop(optional, into)]
    label: Option<String>,
    /// A handle the host keeps to switch documents, save them and reach
    /// their editors; see [`DocumentsHandle`].
    #[prop(optional)]
    handle: Option<DocumentsHandle>,
) -> impl IntoView {
    let documents_view = Documents::new(
        Callbacks {
            on_select,
            on_close,
            on_add,
            on_change,
            on_save,
            on_dirty_change,
            on_edit_as_change,
            on_sync_change,
        },
        settings,
    );
    if let Some(id) = initial_active {
        documents_view.active.set(Some(id));
    }
    documents_view.sync(&documents.get_untracked());
    Effect::new(move |first: Option<()>| {
        let list = documents.get();
        // The first list was taken above, before rendering.
        if first.is_some() {
            untrack(|| documents_view.sync(&list));
        }
    });
    if let Some(handle) = handle {
        handle.attach(documents_view);
    }
    let root_class = format!("config-editor-documents {}", class.unwrap_or_default());
    let keydown = move |event: web_sys::KeyboardEvent| {
        adding::on_root_input(documents_view, &event);
        keys::on_root_keydown(documents_view, &event);
        if let Some(root) = documents_view.root.get_untracked() {
            crate::page_scroll::keep_inside(&root, &event);
        }
    };
    let mousedown = move |event: web_sys::MouseEvent| adding::on_root_input(documents_view, &event);
    let focusout =
        move |event: web_sys::FocusEvent| adding::on_root_focusout(documents_view, &event);
    let panels = move || documents_view.entries.get();
    let panel =
        move |entry: Entry| view! { <DocumentPanel documents=documents_view entry=entry /> };
    let empty = move || documents_view.entries.with(Vec::is_empty);
    let theme = Memo::new(move |_| settings.with(|settings| settings.theme));
    let style = move || settings.with(|settings| settings.colors.declarations());
    let styled = Memo::new(move |_| settings.with(|settings| settings.styled));

    view! {
        // Code reads left to right, in a right-to-left page too: the tabs,
        // their arrow keys and every editor in it.
        <div
            class=root_class
            dir="ltr"
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            style=cssom(style)
            node_ref=documents_view.root
            on:keydown=keydown
            on:mousedown=mousedown
            on:focusout=focusout
        >
            {move || styled.get().then(|| view! { <style>{PRESET_CSS}</style> })}
            <TabStrip documents=documents_view label=label />
            <div class="config-editor-documents__panels">
                <KeyedList each=panels key=|entry| entry.serial children=panel />
                <Show when=empty>
                    <p class="config-editor-documents__empty">{documents_view.text(|strings| &strings.no_documents)}</p>
                </Show>
            </div>
        </div>
    }
}
