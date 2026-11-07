//! The built-in toolbar: file name, schema state and language.

use leptos::prelude::*;

use super::super::state::EditorState;

#[component]
pub(crate) fn Toolbar(state: EditorState, file_name: String, has_schema: bool) -> impl IntoView {
    view! {
        <div class="config-editor__toolbar">
            <div class="config-editor__file">
                <span class="config-editor__file-icon">{move || state.sync.edit_language.get().file_icon()}</span>
                <span>{file_name}</span>
            </div>
            <div class="config-editor__toolbar-meta">
                {has_schema.then(|| view! {
                    <span class="config-editor__schema-state">
                        <span class="config-editor__schema-check">"✓"</span>
                        {state.text(|strings| &strings.schema_connected)}
                    </span>
                })}
                <span class="config-editor__language">{move || state.label()}</span>
            </div>
        </div>
    }
}
