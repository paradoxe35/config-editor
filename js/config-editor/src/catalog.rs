//! What the editor accepts: language and theme names with what they stand
//! for, color tokens, and the preset stylesheet.

use config_editor_leptos::{EditorTheme, Language, PRESET_CSS, ThemeColor};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::languages::{REGISTERED, language_named};
use crate::typings::{LanguageInfoResult, NameResult, Names, ThemeInfoResult};
use crate::values::{text_arg, to_js};

/// A language's name in JavaScript: its label, lowercased (`"yaml"`, or a
/// registered `"env"`), as `languages()` lists it and callbacks report it.
pub(crate) fn language_name(language: Language) -> String {
    language.label().to_lowercase()
}

pub(crate) fn theme_named(name: &str) -> Result<EditorTheme, JsError> {
    name.parse::<EditorTheme>()
        .map_err(|error| JsError::new(&error.to_string()))
}

/// Names of the built-in and registered languages.
#[wasm_bindgen]
pub fn languages() -> Names {
    let mut names: Vec<String> = Language::ALL.iter().copied().map(language_name).collect();
    REGISTERED.with(|languages| names.extend(languages.borrow().keys().cloned()));
    to_js(&names).unchecked_into()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LanguageInfoOut {
    name: String,
    label: &'static str,
    icon: &'static str,
    extensions: &'static [&'static str],
    default_file_name: String,
}

/// What a language name (or file extension, as `language` takes) stands
/// for: its canonical name, label, toolbar icon, extensions and the file
/// name the toolbar shows when none is given. Throws on an unknown name.
#[wasm_bindgen(js_name = languageInfo)]
pub fn language_info(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] name: JsValue,
) -> Result<LanguageInfoResult, JsError> {
    let language = language_named(&text_arg(&name, "language")?)?;
    Ok(to_js(&LanguageInfoOut {
        name: language_name(language),
        label: language.label(),
        icon: language.file_icon(),
        extensions: language.extensions(),
        default_file_name: language.default_file_name(),
    })
    .unchecked_into())
}

/// The language of a file, from its extension (`"config/app.yml"` →
/// `"yaml"`), registered languages included; `null` when none claims it.
#[wasm_bindgen(js_name = languageForFile)]
pub fn language_for_file(
    #[wasm_bindgen(unchecked_param_type = "string")] path: JsValue,
) -> Result<NameResult, JsError> {
    let path = text_arg(&path, "path")?;
    let registered = path.rsplit_once('.').and_then(|(_, extension)| {
        REGISTERED.with(|languages| {
            languages.borrow().values().copied().find(|language| {
                language
                    .extensions()
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(extension))
            })
        })
    });
    Ok(registered
        .or_else(|| Language::from_path(&path))
        .map_or(JsValue::NULL, |language| {
            JsValue::from_str(&language_name(language))
        })
        .unchecked_into())
}

/// Slugs of the bundled themes.
#[wasm_bindgen]
pub fn themes() -> Names {
    let names: Vec<&str> = EditorTheme::ALL
        .iter()
        .map(|theme| theme.as_str())
        .collect();
    to_js(&names).unchecked_into()
}

#[derive(Serialize)]
struct ThemeInfoOut {
    name: &'static str,
    label: &'static str,
}

/// A theme's slug and display name (`"nord"` → `{ name: "nord", label:
/// "Nord" }`), for a picker. Throws on an unknown theme.
#[wasm_bindgen(js_name = themeInfo)]
pub fn theme_info(
    #[wasm_bindgen(unchecked_param_type = "ThemeName")] name: JsValue,
) -> Result<ThemeInfoResult, JsError> {
    let theme = theme_named(&text_arg(&name, "theme")?)?;
    Ok(to_js(&ThemeInfoOut {
        name: theme.as_str(),
        label: theme.label(),
    })
    .unchecked_into())
}

/// Token names accepted in `colors`.
#[wasm_bindgen(js_name = colorTokens)]
pub fn color_tokens() -> Names {
    let names: Vec<&str> = ThemeColor::ALL
        .iter()
        .filter_map(|color| color.css_var().strip_prefix("--config-editor-"))
        .collect();
    to_js(&names).unchecked_into()
}

/// The stylesheet the editor injects (layout and every bundled theme), for
/// a page that sets `styled: false` and serves or adapts it itself.
#[wasm_bindgen(js_name = presetCss)]
pub fn preset_css() -> String {
    PRESET_CSS.to_string()
}
