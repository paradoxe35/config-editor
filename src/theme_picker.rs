//! The theme select of both demo pages: switches the `theme` setting live.

use config_editor_leptos::{EditorSettings, EditorTheme};
use leptos::prelude::*;

/// Sets the theme the select picked.
fn pick(settings: RwSignal<EditorSettings>, event: &leptos::ev::Event) {
    let theme = event_target_value(event).parse().unwrap_or_default();
    settings.update(|settings| settings.theme = theme);
}

#[component]
pub fn ThemePicker(settings: RwSignal<EditorSettings>) -> impl IntoView {
    let current = settings.with_untracked(|settings| settings.theme);
    let option = move |theme: &EditorTheme| {
        view! { <option value=theme.as_str() selected=*theme == current>{theme.label()}</option> }
    };
    view! {
        <label class="demo__picker">
            <span class="demo__picker-label">"Theme"</span>
            <select
                class="demo__select demo__theme-select"
                aria-label="Editor theme"
                on:change=move |event| pick(settings, &event)
            >
                {EditorTheme::ALL.iter().map(option).collect_view()}
            </select>
        </label>
    }
}
