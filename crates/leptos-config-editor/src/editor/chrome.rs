use super::theme::EditorTheme;
use leptos::prelude::*;

/// Optional toolbar shell for hosts that want the built-in file/schema bar.
/// For custom page chrome, set `show_toolbar=false` on [`super::YamlEditor`]
/// and compose this component (or plain markup) in the host instead.
#[component]
pub fn YamlEditorToolbar(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
) -> impl IntoView {
    view! {
        <div
            class=format!("yaml-editor yaml-editor__toolbar {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
        >
            {children()}
        </div>
    }
}

/// Optional status-footer shell for hosts that want the built-in status bar.
/// For custom page chrome, set `show_footer=false` on [`super::YamlEditor`]
/// and compose this component (or plain markup) in the host instead.
#[component]
pub fn YamlEditorFooter(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
) -> impl IntoView {
    view! {
        <div
            class=format!("yaml-editor yaml-editor__status {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
        >
            {children()}
        </div>
    }
}
