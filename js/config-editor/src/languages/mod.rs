//! Language names, and languages written in JavaScript (`registerLanguage`).

use std::cell::RefCell;
use std::collections::HashMap;

use config_editor_leptos::Language;
use config_editor_leptos::language::{LanguageConfig, LanguageInfo};
use js_sys::{Array, Function};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::options::warn_unknown_keys;
use crate::typings::CustomLanguage;
use crate::values::{field, string_field};

mod assist;
pub(crate) mod interned;
mod js_language;
mod ranges;
mod syntax;

use js_language::JsLanguage;

thread_local! {
    /// Languages registered from JavaScript, by lowercase label.
    pub(crate) static REGISTERED: RefCell<HashMap<String, Language>> = RefCell::new(HashMap::new());
    /// The same languages as they were registered, to take a new definition.
    static OWN: RefCell<HashMap<String, &'static JsLanguage>> = RefCell::new(HashMap::new());
}

pub(crate) fn language_named(name: &str) -> Result<Language, JsError> {
    let key = name.trim().to_lowercase();
    if let Some(language) = REGISTERED.with(|languages| languages.borrow().get(&key).copied()) {
        return Ok(language);
    }
    // A registered language's file extension names it too, as `yml` does.
    if let Some(language) = registered_for_extension(key.trim_start_matches('.')) {
        return Ok(language);
    }
    name.parse::<Language>()
        .map_err(|error| JsError::new(&error.to_string()))
}

/// The registered language claiming `extension`, if any.
fn registered_for_extension(extension: &str) -> Option<Language> {
    REGISTERED.with(|languages| {
        languages.borrow().values().copied().find(|language| {
            language
                .extensions()
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
    })
}

/// The language of a file, from its extension (`"config/app.yml"` →
/// YAML), registered languages first; `None` when none claims it.
pub(crate) fn language_for_path(path: &str) -> Option<Language> {
    path.rsplit_once('.')
        .and_then(|(_, extension)| registered_for_extension(extension))
        .or_else(|| Language::from_path(path))
}

/// `edit` if editing a `host` document as it can be saved: both languages
/// must write documents (`edit` to show it, `host` to save it), unless
/// `edit` is the host's own.
pub(crate) fn writable_edit_as(host: Language, edit: Language) -> Result<Language, JsError> {
    let languages: &[Language] = if edit == host { &[] } else { &[edit, host] };
    for language in languages {
        language
            .serialize(&Value::Object(serde_json::Map::new()))
            .map_err(|error| JsError::new(&format!("editAs: {error}")))?;
    }
    Ok(edit)
}

/// Keys a `registerLanguage` definition can hold.
const DEFINITION_KEYS: &[&str] = &[
    "label",
    "icon",
    "extensions",
    "parse",
    "serialize",
    "keySpan",
    "valueSpan",
    "highlight",
    "comments",
    "brackets",
    "separators",
    "mapBrackets",
    "listBrackets",
    "itemSeparator",
    "itemMarker",
    "literalQuotes",
    "writtenSeparator",
    "contextAt",
    "insertKey",
    "keyAt",
    "multilineRanges",
];

/// A list of strings, or one string, as `name` (absent is empty).
fn string_list(value: &JsValue, name: &str) -> Result<Vec<String>, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if let Some(text) = value.as_string() {
        if text.is_empty() {
            return Err(JsError::new(&format!("{name} must be a non-empty string")));
        }
        return Ok(vec![text]);
    }
    if !Array::is_array(value) {
        return Err(JsError::new(&format!(
            "{name} must be a string or an array of strings"
        )));
    }
    Array::from(value)
        .iter()
        .map(|item| {
            item.as_string()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| JsError::new(&format!("{name} must hold non-empty strings")))
        })
        .collect()
}

/// The declarative part of a JavaScript language definition.
fn language_config(definition: &JsValue) -> Result<LanguageConfig, JsError> {
    let comments = field(definition, "comments");
    let (line_comments, block_comment) = if comments.is_undefined() || comments.is_null() {
        (Vec::new(), None)
    } else if !comments.is_object() || Array::is_array(&comments) {
        return Err(JsError::new(
            "comments must be an object: { lineComment?, blockComment? }",
        ));
    } else {
        warn_unknown_keys(
            &comments,
            &["lineComment", "blockComment"],
            "comments field",
        );
        let block = string_list(&field(&comments, "blockComment"), "comments.blockComment")?;
        let block = match block.as_slice() {
            [] => None,
            [open, close] => Some((interned::text(open), interned::text(close))),
            _ => return Err(JsError::new("comments.blockComment must be [open, close]")),
        };
        (
            string_list(&field(&comments, "lineComment"), "comments.lineComment")?,
            block,
        )
    };
    let separators_value = field(definition, "separators");
    if separators_value.is_string() {
        return Err(JsError::new(
            "separators must be an array of strings, e.g. [\"=\"]",
        ));
    }
    let separators = string_list(&separators_value, "separators")?;
    let brackets_value = field(definition, "brackets");
    let mut brackets = Vec::new();
    if !(brackets_value.is_undefined() || brackets_value.is_null()) {
        if !Array::is_array(&brackets_value) {
            return Err(JsError::new(
                "brackets must be an array of [open, close] pairs",
            ));
        }
        for pair in Array::from(&brackets_value).iter() {
            let Some(pair) = syntax::pair(&pair) else {
                return Err(JsError::new(
                    "brackets must be [open, close] pairs of single characters",
                ));
            };
            brackets.push(pair);
        }
    }
    const QUOTES: &[&str] = &["\"", "'"];
    let separators = interned::texts(&separators);
    Ok(LanguageConfig {
        line_comments: interned::texts(&line_comments),
        block_comment,
        brackets: interned::pairs(brackets),
        separators,
        written_separator: syntax::written_separator(definition, separators)?,
        quotes: QUOTES,
        literal_quotes: syntax::literal_quotes(definition, QUOTES)?,
        map_brackets: syntax::bracket_member(definition, "mapBrackets")?,
        list_brackets: syntax::bracket_member(definition, "listBrackets")?,
        item_separator: syntax::item_separator(definition)?,
        item_marker: syntax::item_marker(definition)?,
    })
}

/// Registers a language implemented in JavaScript and returns its name for
/// `language` / `editAs` (its label, lowercased). Registering the same label
/// again replaces its definition, in the editors already open too, and
/// takes no more memory.
#[cfg(not(target_feature = "atomics"))]
#[wasm_bindgen(js_name = registerLanguage)]
pub fn register_language(definition: CustomLanguage) -> Result<String, JsError> {
    let definition: JsValue = definition.into();
    let label = string_field(&definition, "label")
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .ok_or_else(|| JsError::new("a language needs a label"))?;
    if label.parse::<Language>().is_ok() {
        return Err(JsError::new(&format!("\"{label}\" is a built-in language")));
    }
    warn_unknown_keys(&definition, DEFINITION_KEYS, "language field");
    if field(&definition, "parse").dyn_into::<Function>().is_err() {
        return Err(JsError::new("a language needs parse(text)"));
    }
    for name in [
        "serialize",
        "keySpan",
        "valueSpan",
        "highlight",
        "contextAt",
        "insertKey",
        "keyAt",
        "multilineRanges",
    ] {
        let value = field(&definition, name);
        if !value.is_undefined() && !value.is_null() && !value.is_function() {
            return Err(JsError::new(&format!("{name} must be a function")));
        }
    }
    let icon_value = field(&definition, "icon");
    let icon = if icon_value.is_undefined() || icon_value.is_null() {
        label.chars().next().map(String::from).unwrap_or_default()
    } else {
        match icon_value.as_string() {
            Some(icon)
                if (1..=2).contains(&icon.chars().count())
                    && icon.chars().all(char::is_alphanumeric) =>
            {
                icon
            }
            _ => return Err(JsError::new("icon must be one or two letters")),
        }
    };
    let extensions_value = field(&definition, "extensions");
    let extensions: Vec<String> = if extensions_value.is_undefined() || extensions_value.is_null() {
        Vec::new()
    } else if Array::is_array(&extensions_value) {
        Array::from(&extensions_value)
            .iter()
            .map(|extension| {
                // Written without the dot (`env`); a built-in's is taken.
                let extension = extension
                    .as_string()
                    .map(|text| text.trim().trim_start_matches('.').to_lowercase())
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| JsError::new("extensions must hold non-empty strings"))?;
                if extension.parse::<Language>().is_ok() {
                    return Err(JsError::new(&format!(
                        "extension \"{extension}\" already names a built-in language"
                    )));
                }
                Ok(extension)
            })
            .collect::<Result<_, _>>()?
    } else {
        return Err(JsError::new(
            "extensions must be an array of strings, e.g. [\"env\"]",
        ));
    };
    let name = label.to_lowercase();
    // A name or extension another registered language already answers to
    // would be taken from it, or never reach this one.
    REGISTERED.with(|languages| {
        for (other, language) in languages
            .borrow()
            .iter()
            .filter(|(other, _)| **other != name)
        {
            if language.extensions().contains(&name.as_str()) {
                return Err(JsError::new(&format!(
                    "\"{name}\" is already an extension of the registered language \"{other}\""
                )));
            }
            if let Some(extension) = extensions.iter().find(|extension| {
                *extension == other.as_str() || language.extensions().contains(&extension.as_str())
            }) {
                return Err(JsError::new(&format!(
                    "extension \"{extension}\" already names the registered language \"{other}\""
                )));
            }
        }
        Ok(())
    })?;
    let config = language_config(&definition)?;
    let info = LanguageInfo {
        label: interned::text(&label),
        icon: interned::text(&icon),
        extensions: interned::texts(&extensions),
    };
    // The label's language, kept for good: registered again, it takes the
    // new definition (editors already open follow it).
    let known = OWN.with(|own| own.borrow().get(&name).copied());
    match known {
        Some(language) => language.replace(definition, info, config),
        None => {
            let language: &'static JsLanguage =
                Box::leak(Box::new(JsLanguage::new(definition, info, config)));
            OWN.with(|own| own.borrow_mut().insert(name.clone(), language));
            REGISTERED.with(|languages| {
                languages
                    .borrow_mut()
                    .insert(name.clone(), Language::new(language));
            });
        }
    }
    Ok(name)
}
