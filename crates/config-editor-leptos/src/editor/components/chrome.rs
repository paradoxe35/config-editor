use super::super::theme::{EditorTheme, ThemeChrome, ThemeColors, root_style};
use crate::cssom::{Declarations, cssom};
use leptos::prelude::*;

/// Toolbar container styled to match the editor.
///
/// Use it to build your own toolbar with `show_toolbar=false` on
/// [`ConfigEditor`](crate::ConfigEditor).
#[component]
pub fn ConfigEditorToolbar(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
    /// Colors overriding the theme's, as on [`ConfigEditor`](crate::ConfigEditor).
    #[prop(optional, into)]
    colors: MaybeProp<ThemeColors>,
    /// Fonts, spacing and borders overriding the preset's, as on
    /// [`ConfigEditor`](crate::ConfigEditor).
    #[prop(optional, into)]
    chrome: MaybeProp<ThemeChrome>,
) -> impl IntoView {
    view! {
        <div
            class=format!("config-editor config-editor__toolbar {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            style=cssom(move || style(colors, chrome))
        >
            {children()}
        </div>
    }
}

/// Status-bar container styled to match the editor.
///
/// Use it to build your own footer with `show_footer=false` on
/// [`ConfigEditor`](crate::ConfigEditor).
#[component]
pub fn ConfigEditorFooter(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
    /// Colors overriding the theme's, as on [`ConfigEditor`](crate::ConfigEditor).
    #[prop(optional, into)]
    colors: MaybeProp<ThemeColors>,
    /// Fonts, spacing and borders overriding the preset's, as on
    /// [`ConfigEditor`](crate::ConfigEditor).
    #[prop(optional, into)]
    chrome: MaybeProp<ThemeChrome>,
) -> impl IntoView {
    view! {
        <div
            class=format!("config-editor config-editor__status {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            style=cssom(move || style(colors, chrome))
        >
            {children()}
        </div>
    }
}

/// The root's inline style from the `colors` and `chrome` props.
fn style(colors: MaybeProp<ThemeColors>, chrome: MaybeProp<ThemeChrome>) -> Declarations {
    colors.with(|colors| {
        chrome.with(|chrome| {
            root_style(
                colors.as_ref().unwrap_or(&ThemeColors::new()),
                chrome.as_ref().unwrap_or(&ThemeChrome::new()),
            )
        })
    })
}
