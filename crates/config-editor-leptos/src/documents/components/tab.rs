//! One document's tab: its file icon and name, the dirty dot, and the close
//! button (only while the document may be closed).

use leptos::prelude::*;
use web_sys::{KeyboardEvent, MouseEvent};

use super::super::entries::{Documents, Entry, Focus};
use super::super::keys;
use super::super::tab_names::{TabLabel, tab_label};
use crate::cssom::{Declarations, cssom};
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
        conflicted,
        read_only,
        read_only_reason,
        ..
    } = entry;
    let icon = entry.setup.language.file_icon();
    let strings = documents.follows.strings.read();
    let names = documents.follows.names.read();
    // What the tab shows of the name (the title and accessible name keep
    // it whole).
    let name = move || {
        let name = file_name.get();
        names.with(|names| tab_label(&name, names))
    };
    // Room for the letters that tell it from the tabs with the same
    // file's name, and the ellipsis after them.
    let style = move || {
        let kept = name().kept.chars().count();
        (kept > 0).then(|| {
            Declarations::new().set("--config-editor-tab-kept", format!("calc({kept}ch + 1em)"))
        })
    };
    let accessible_name = move || {
        let mut name = file_name.get();
        strings.with(|strings| {
            if dirty.get() {
                name.push_str(&format!(", {}", strings.unsaved));
            }
            if conflicted.get() {
                name.push_str(&format!(", {}", strings.baseline_changed));
            }
            if read_only.get() {
                name.push_str(&format!(", {}", strings.read_only_tab));
            }
        });
        name
    };
    // Why it is read-only, while it is and the host said why.
    let reason = move || {
        read_only
            .get()
            .then(|| read_only_reason.get())
            .flatten()
            .filter(|reason| !reason.trim().is_empty())
    };
    let title = move || {
        let name = file_name.get();
        match reason() {
            Some(reason) => strings.with(|strings| {
                fill(
                    &strings.read_only_tab_title,
                    &[("name", &name), ("reason", &reason)],
                )
            }),
            None => name,
        }
    };
    let reason_id = format!("{}-reason", entry.tab_id(documents));
    let described_by = {
        let reason_id = reason_id.clone();
        move || reason().is_some().then(|| reason_id.clone())
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
            class="config-editor-documents__tab"
            // One toggle per state: a switch of documents re-runs only the
            // one that follows the selection. One closure reading all three
            // would unsubscribe and resubscribe from `dirty` and `read_only`
            // (which the editor reads too) on every switch, and that churn
            // grows their subscriber tables at some later switch.
            class:config-editor-documents__tab-active=move || selected.get()
            class:config-editor-documents__tab-dirty=move || dirty.get()
            class:config-editor-documents__tab-readonly=move || read_only.get()
            style=cssom(style)
            aria-controls=entry.panel_id(documents)
            aria-selected=move || selected.get().to_string()
            aria-label=accessible_name
            tabindex=move || if selected.get() { "0" } else { "-1" }
            title=title
            aria-describedby=described_by
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
            <span class="config-editor__sr-only" id=reason_id>{reason}</span>
            {marker}
        </div>
    }
}

/// A tab's name: the letters that tell it from tabs with the same file's
/// name, kept whole; its folder (in its own span, which a narrow strip
/// leaves out) and the start, which a narrow tab cuts; then the end, kept
/// whole.
fn name_view(
    TabLabel {
        folder,
        kept,
        start,
        end,
    }: TabLabel,
) -> impl IntoView {
    let kept = (!kept.is_empty())
        .then(|| view! { <span class="config-editor-documents__name-kept">{kept}</span> });
    let end = (!end.is_empty())
        .then(|| view! { <span class="config-editor-documents__name-end">{end}</span> });
    view! {
        {kept}
        <span class="config-editor-documents__name-start">
            {folder.map(|folder| view! { <span class="config-editor-documents__name-folder">{folder}</span> })}
            {start}
        </span>
        {end}
    }
}
