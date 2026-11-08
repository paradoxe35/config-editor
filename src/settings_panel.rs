//! A checkbox per on/off setting of [`EditorSettings`], switching it live
//! (the e2e tests toggle them and check the caret and undo stay). The list
//! comes from the settings' own JSON keys, so a new setting shows up here
//! by itself.
//!
//! A button opens the list, not a `<details>`: a closed `<details>`
//! anywhere on the page makes Chrome select text in a large textarea in
//! quadratic time (see the crate README, "Large documents"). The open list
//! stays inside the window at every width, and Escape or a click outside
//! closes it.

use config_editor_leptos::EditorSettings;
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;

/// Room kept between the open list and the window's edges, in pixels.
const EDGE: f64 = 8.0;

/// The on/off settings' JSON keys and values.
fn switches(settings: &EditorSettings) -> Vec<(String, bool)> {
    let serde_json::Value::Object(fields) = serde_json::to_value(settings).unwrap_or_default()
    else {
        return Vec::new();
    };
    fields
        .into_iter()
        .filter_map(|(key, value)| value.as_bool().map(|on| (key, on)))
        .collect()
}

/// `settings` with the switch `key` set to `on`.
fn with_switch(settings: &EditorSettings, key: &str, on: bool) -> EditorSettings {
    let mut json = serde_json::to_value(settings).unwrap_or_default();
    json[key] = serde_json::Value::Bool(on);
    serde_json::from_value(json).unwrap_or_else(|_| settings.clone())
}

/// One setting's checkbox.
fn switch(settings: RwSignal<EditorSettings>, key: String) -> impl IntoView {
    let flip = key.clone();
    let toggle = move |event| {
        let on = event_target_checked(&event);
        settings.update(|current| *current = with_switch(current, &flip, on));
    };
    let checked = {
        let key = key.clone();
        move || {
            settings.with(|current| {
                switches(current)
                    .into_iter()
                    .any(|(name, on)| name == key && on)
            })
        }
    };
    view! {
        <label class="demo__picker demo__setting">
            <input type="checkbox" data-setting=key.clone() prop:checked=checked on:change=toggle />
            {key}
        </label>
    }
}

/// How far to move the open `list` sideways so it fits in the window: it
/// opens under the button's right end, and moves right when that would put
/// its left edge off screen (a narrow window), left when its right edge.
fn fitting_shift(list: &web_sys::Element, current: f64) -> f64 {
    let Some(width) = window().inner_width().ok().and_then(|width| width.as_f64()) else {
        return 0.0;
    };
    let rect = list.get_bounding_client_rect();
    let left = rect.left() - current;
    let right = left + rect.width();
    if left < EDGE {
        EDGE - left
    } else if right > width - EDGE {
        (width - EDGE - right).max(EDGE - left)
    } else {
        0.0
    }
}

/// Closes the list on Escape (focus back on its button) and on a press
/// outside it, while it is open.
fn close_from_outside(open: RwSignal<bool>, root: NodeRef<leptos::html::Div>) {
    let escape = window_event_listener(leptos::ev::keydown, move |event| {
        if open.get_untracked() && event.key() == "Escape" {
            open.set(false);
            if let Some(button) = root
                .get_untracked()
                .and_then(|root| root.query_selector(".demo__settings-toggle").ok().flatten())
                .and_then(|button| button.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let _ = button.focus();
            }
        }
    });
    let outside = window_event_listener(leptos::ev::pointerdown, move |event| {
        if !open.get_untracked() {
            return;
        }
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Node>().ok());
        let inside = root
            .get_untracked()
            .is_some_and(|root| root.contains(target.as_ref()));
        if !inside {
            open.set(false);
        }
    });
    on_cleanup(move || {
        escape.remove();
        outside.remove();
    });
}

#[component]
pub fn SettingsPanel(settings: RwSignal<EditorSettings>) -> impl IntoView {
    let keys: Vec<String> = switches(&settings.get_untracked())
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    let open = RwSignal::new(false);
    let shift = RwSignal::new(0.0_f64);
    let root = NodeRef::<leptos::html::Div>::new();
    let list = NodeRef::<leptos::html::Div>::new();
    // Placed in the frame the list shows in (when it has a size).
    Effect::new(move |_| {
        if open.get() {
            request_animation_frame(move || {
                if let Some(list) = list.get_untracked() {
                    shift.set(fitting_shift(&list, shift.get_untracked()));
                }
            });
        }
    });
    close_from_outside(open, root);
    let on_toggle = move |_| toggle(open);
    let placement = move || format!("translateX({}px)", shift.get());
    view! {
        <div class="demo__settings" node_ref=root>
            <button
                class="demo__settings-toggle"
                type="button"
                aria-expanded=move || open.get().to_string()
                aria-controls="demo-settings-list"
                on:click=on_toggle
            >
                "Settings"
            </button>
            <div
                id="demo-settings-list"
                class="demo__settings-list"
                node_ref=list
                style:transform=placement
                hidden=move || !open.get()
            >
                {keys.into_iter().map(|key| switch(settings, key)).collect_view()}
            </div>
        </div>
    }
}

/// Opens or closes a disclosure.
pub fn toggle(open: RwSignal<bool>) {
    open.update(|open| *open = !*open);
}
