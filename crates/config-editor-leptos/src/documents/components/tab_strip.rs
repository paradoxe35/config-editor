//! The tab strip: a WAI-ARIA tab list that scrolls sideways when the tabs
//! overflow, keeping the active one in view.

use leptos::prelude::*;
use web_sys::WheelEvent;

use super::super::entries::{Documents, Entry};
use super::DocumentTab;
use crate::reactive::KeyedList;

#[component]
pub(in crate::documents) fn TabStrip(documents: Documents, label: Option<String>) -> impl IntoView {
    reveal_active_tab(documents);
    // A vertical wheel scrolls the strip sideways, as in VS Code.
    let wheel = move |event: WheelEvent| {
        let Some(strip) = documents.strip.get_untracked() else {
            return;
        };
        let overflows = strip.scroll_width() > strip.client_width();
        if overflows && event.delta_x() == 0.0 && event.delta_y() != 0.0 {
            event.prevent_default();
            strip.set_scroll_left(strip.scroll_left() + event.delta_y() as i32);
        }
    };
    let tabs = move || documents.entries.get();
    let default_label = documents.text(|strings| &strings.tab_list);
    let label = move || label.clone().unwrap_or_else(default_label);
    let tab = move |entry: Entry| view! { <DocumentTab documents=documents entry=entry /> };

    view! {
        <div
            class="config-editor-documents__tabs"
            role="tablist"
            aria-label=label
            aria-orientation="horizontal"
            node_ref=documents.strip
            on:wheel=wheel
        >
            <KeyedList each=tabs key=|entry| entry.serial children=tab />
        </div>
    }
}

/// Scrolls the strip so the active tab shows whole, after each switch.
fn reveal_active_tab(documents: Documents) {
    Effect::new(move |_| {
        let Some(id) = documents.active.get() else {
            return;
        };
        request_animation_frame(move || {
            let (Some(strip), Some(tab)) =
                (documents.strip.get_untracked(), documents.tab_element(&id))
            else {
                return;
            };
            let left = tab.offset_left();
            let right = left + tab.offset_width();
            let view_left = strip.scroll_left();
            let view_right = view_left + strip.client_width();
            if left < view_left {
                strip.set_scroll_left(left);
            } else if right > view_right {
                strip.set_scroll_left(right - strip.client_width());
            }
        });
    });
}
