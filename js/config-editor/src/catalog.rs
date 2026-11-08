//! What the editor accepts: language and theme names with what they stand
//! for, color tokens, and the preset stylesheet.

use config_editor_leptos::{EditorTheme, Language, PRESET_CSS};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::languages::{REGISTERED, language_for_path};
use crate::typings::{LanguageInfos, NameResult, ThemeInfos};
use crate::values::{text_arg, to_js};

/// A language's name in JavaScript: its label, lowercased (`"yaml"`, or a
/// registered `"env"`), as `languages()` lists it and callbacks report it.
pub(crate) fn language_name(language: Language) -> String {
    language.label().to_lowercase()
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

impl From<Language> for LanguageInfoOut {
    fn from(language: Language) -> Self {
        Self {
            name: language_name(language),
            label: language.label(),
            icon: language.file_icon(),
            extensions: language.extensions(),
            default_file_name: language.default_file_name(),
        }
    }
}

/// The built-in and registered languages, each with its canonical name
/// (what `language` and `editAs` take), label, toolbar icon, extensions and
/// the file name the toolbar shows when none is given.
#[wasm_bindgen]
pub fn languages() -> LanguageInfos {
    let mut found: Vec<LanguageInfoOut> = Language::ALL
        .iter()
        .copied()
        .map(LanguageInfoOut::from)
        .collect();
    REGISTERED.with(|languages| {
        found.extend(
            languages
                .borrow()
                .values()
                .copied()
                .map(LanguageInfoOut::from),
        );
    });
    to_js(&found).unchecked_into()
}

/// The language of a file, from its extension (`"config/app.yml"` →
/// `"yaml"`), registered languages included; `null` when none claims it.
#[wasm_bindgen(js_name = languageForFile)]
pub fn language_for_file(
    #[wasm_bindgen(unchecked_param_type = "string")] path: JsValue,
) -> Result<NameResult, JsError> {
    let path = text_arg(&path, "path")?;
    Ok(language_for_path(&path)
        .map_or(JsValue::NULL, |language| {
            JsValue::from_str(&language_name(language))
        })
        .unchecked_into())
}

#[derive(Serialize)]
struct ThemeInfoOut {
    name: &'static str,
    label: &'static str,
    dark: bool,
}

/// The bundled themes, for a picker: each one's slug (what `theme` takes),
/// display name, and whether it is dark.
#[wasm_bindgen]
pub fn themes() -> ThemeInfos {
    let themes: Vec<ThemeInfoOut> = EditorTheme::ALL
        .iter()
        .map(|theme| ThemeInfoOut {
            name: theme.as_str(),
            label: theme.label(),
            // Each theme's stylesheet builds on one of the four families.
            dark: !matches!(theme.css_family(), "vscode-light" | "obsidian-light"),
        })
        .collect();
    to_js(&themes).unchecked_into()
}

/// The stylesheet the editor injects (layout and every bundled theme), for
/// a page that sets `styled: false` and serves or adapts it itself.
#[wasm_bindgen(js_name = presetCss)]
pub fn preset_css() -> String {
    PRESET_CSS.to_string()
}
