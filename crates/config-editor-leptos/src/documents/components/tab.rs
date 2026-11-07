//! One document's tab: its file icon and name, the dirty dot, and the close
//! button.

use leptos::prelude::*;
use web_sys::{KeyboardEvent, MouseEvent};

use super::super::entries::{Documents, Entry, Focus};
use super::super::{keys, texts};

#[component]
pub(in crate::documents) fn DocumentTab(documents: Documents, entry: Entry) -> impl IntoView {
    let id = StoredValue::new(entry.id.clone());
    let selected = Memo::new(move |_| {
        documents
            .active
            .with(|active| id.with_value(|id| active.as_deref() == Some(id.as_str())))
    });
    let closable = documents.callbacks.on_close.is_some();
    let Entry {
        file_name,
        dirty,
        read_only,
        ..
    } = entry;
    let icon = entry.setup.language.file_icon();
    let accessible_name = move || {
        let mut name = file_name.get();
        if dirty.get() {
            name.push_str(&format!(", {}", texts::UNSAVED));
        }
        if read_only.get() {
            name.push_str(&format!(", {}", texts::READ_ONLY));
        }
        name
    };
    let close_label = move || format!("{} {}", texts::CLOSE_LABEL, file_name.get());

    let select = move |_: MouseEvent| {
        id.with_value(|id| documents.select(id, Focus::Keep));
    };
    let middle_click = move |event: MouseEvent| {
        if event.button() == 1 && closable {
            event.prevent_default();
            id.with_value(|id| documents.request_close(id));
        }
    };
    // A middle press would otherwise start the browser's autoscroll.
    let middle_press = move |event: MouseEvent| {
        if event.button() == 1 && closable {
            event.prevent_default();
        }
    };
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
    let marker = if closable {
        view! {
            <button
                class="config-editor-documents__close"
                type="button"
                tabindex="-1"
                title=texts::CLOSE_TITLE
                aria-label=close_label
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
                <span class="config-editor-documents__dirty-dot" title=texts::UNSAVED>"●"</span>
            </span>
        }
        .into_any()
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
            <span class="config-editor-documents__name">{move || file_name.get()}</span>
            {marker}
        </div>
    }
}
