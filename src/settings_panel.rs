//! A checkbox per on/off setting of [`EditorSettings`], switching it live
//! (the e2e tests toggle them and check the caret and undo stay). The list
//! comes from the settings' own JSON keys, so a new setting shows up here
//! by itself.

use config_editor_leptos::EditorSettings;
use leptos::prelude::*;

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

#[component]
pub fn SettingsPanel(settings: RwSignal<EditorSettings>) -> impl IntoView {
    let keys: Vec<String> = switches(&settings.get_untracked())
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    let switch = move |key: String| {
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
    };
    view! {
        <details class="demo__settings">
            <summary class="demo__summary">"Settings"</summary>
            <div class="demo__settings-list">{keys.into_iter().map(switch).collect_view()}</div>
        </details>
    }
}
