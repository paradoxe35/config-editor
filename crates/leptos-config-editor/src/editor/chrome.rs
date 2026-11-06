use super::theme::{EditorTheme, ThemeColors};
use leptos::prelude::*;

/// Toolbar container styled to match the editor.
///
/// Use it to build your own toolbar with `show_toolbar=false` on
/// [`ConfigEditor`](super::ConfigEditor).
#[component]
pub fn ConfigEditorToolbar(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
    /// Colors overriding the theme's, as on [`ConfigEditor`](super::ConfigEditor).
    #[prop(optional, into)]
    colors: MaybeProp<ThemeColors>,
) -> impl IntoView {
    view! {
        <div
            class=format!("config-editor config-editor__toolbar {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            style=move || colors.get().map(|colors| colors.to_style())
        >
            {children()}
        </div>
    }
}

/// Status-bar container styled to match the editor.
///
/// Use it to build your own footer with `show_footer=false` on
/// [`ConfigEditor`](super::ConfigEditor).
#[component]
pub fn ConfigEditorFooter(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
    /// Colors overriding the theme's, as on [`ConfigEditor`](super::ConfigEditor).
    #[prop(optional, into)]
    colors: MaybeProp<ThemeColors>,
) -> impl IntoView {
    view! {
        <div
            class=format!("config-editor config-editor__status {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            style=move || colors.get().map(|colors| colors.to_style())
        >
            {children()}
        </div>
    }
}
