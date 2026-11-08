//! One document's tab: its file icon and name, the dirty dot, and the close
//! button (only while the document may be closed).

use leptos::prelude::*;
use web_sys::{KeyboardEvent, MouseEvent};

use super::super::entries::{Documents, Entry, Focus};
use super::super::keys;
use super::super::tab_names::split_name;
use crate::strings::fill;

#[component]
pub(in crate::documents) fn DocumentTab(documents: Documents, entry: Entry) -> impl IntoView {
    let id = StoredValue::new(entry.id.clone());
    let active = documents.follows.active.read();
    let selected = Memo::new(move |_| {
        active.with(|active| id.with_value(|id| active.as_deref() == Some(id.as_str())))
    });
    let closes = {
        let entry = entry.clone();
        Memo::new(move |_| documents.closes(&entry))
    };
    let Entry {
        file_name,
        dirty,
        read_only,
        ..
    } = entry;
    let icon = entry.setup.language.file_icon();
    let strings = documents.follows.strings.read();
    let shared = documents.follows.shared_names.read();
    // The name as its folder and the rest: a narrow strip leaves the
    // folder out (the title and accessible name keep it).
    let name = move || {
        let name = file_name.get();
        shared.with(|shared| {
            let (folder, rest) = split_name(&name, shared);
            (folder.map(str::to_string), rest.to_string())
        })
    };
    let accessible_name = move || {
        let mut name = file_name.get();
        strings.with(|strings| {
            if dirty.get() {
                name.push_str(&format!(", {}", strings.unsaved));
            }
            if read_only.get() {
                name.push_str(&format!(", {}", strings.read_only_tab));
            }
        });
        name
    };
    let close_label =
        move || strings.with(|strings| fill(&strings.close_tab, &[("name", &file_name.get())]));

    let select = move |_: MouseEvent| {
        id.with_value(|id| documents.pick(id, Focus::Keep));
    };
    let middle_click = move |event: MouseEvent| {
        if event.button() == 1 && closes.get_untracked() {
            event.prevent_default();
            id.with_value(|id| documents.request_close(id));
        }
    };
    // A middle press would otherwise start the browser's autoscroll.
    let middle_press = move |event: MouseEvent| {
        if event.button() == 1 && closes.get_untracked() {
            event.prevent_default();
        }
    };
    // A press on × leaves the focus where it was (the editor, as in VS
    // Code), not on the button.
    let close_press = move |event: MouseEvent| event.prevent_default();
    let close = move |event: MouseEvent| {
        event.stop_propagation();
        id.with_value(|id| documents.request_close(id));
    };
    let keydown = move |event: KeyboardEvent| {
        id.with_value(|id| keys::on_tab_keydown(documents, id, &event));
    };
    let class = move || {
        let mut class = String::from("config-editor-documents__tab");
        if selected.get() {
            class.push_str(" config-editor-documents__tab-active");
        }
        if dirty.get() {
            class.push_str(" config-editor-documents__tab-dirty");
        }
        if read_only.get() {
            class.push_str(" config-editor-documents__tab-readonly");
        }
        class
    };
    let close_title = documents.text(|strings| &strings.close_tab_title);
    let unsaved_title = documents.text(|strings| &strings.unsaved);
    let marker = move || {
        if closes.get() {
            view! {
                <button
                    class="config-editor-documents__close"
                    type="button"
                    tabindex="-1"
                    title=close_title
                    aria-label=close_label
                    on:mousedown=close_press
                    on:click=close
                >
                    <span class="config-editor-documents__dirty-dot" aria-hidden="true">"●"</span>
                    <span class="config-editor-documents__close-icon" aria-hidden="true">"×"</span>
                </button>
            }
            .into_any()
        } else {
            view! {
                <span class="config-editor-documents__marker" aria-hidden="true">
                    <span class="config-editor-documents__dirty-dot" title=unsaved_title>"●"</span>
                </span>
            }
            .into_any()
        }
    };

    view! {
        <div
            role="tab"
            id=entry.tab_id(documents)
            class=class
            aria-controls=entry.panel_id(documents)
            aria-selected=move || selected.get().to_string()
            aria-label=accessible_name
            tabindex=move || if selected.get() { "0" } else { "-1" }
            title=move || file_name.get()
            data-document-id=entry.id.clone()
            data-dirty=move || dirty.get().to_string()
            on:click=select
            on:mousedown=middle_press
            on:mouseup=middle_click
            on:keydown=keydown
        >
            <span class="config-editor__file-icon config-editor-documents__icon" aria-hidden="true">
                {icon}
            </span>
            <span class="config-editor-documents__name">{move || name_view(name())}</span>
            {marker}
        </div>
    }
}

/// A tab's name: its folder (in its own span), then the rest.
fn name_view((folder, rest): (Option<String>, String)) -> impl IntoView {
    view! {
        {folder.map(|folder| view! { <span class="config-editor-documents__name-folder">{folder}</span> })}
        {rest}
    }
}
