//! Language names, and languages written in JavaScript (`registerLanguage`).

use std::cell::RefCell;
use std::collections::HashMap;

use config_editor_leptos::Language;
use config_editor_leptos::language::{
    ConfigLanguage, LanguageConfig, LanguageInfo, Lexical, ParseError, Span, Token, TokenKind,
    Tokens,
};
use js_sys::{Array, Function};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::options::warn_unknown_keys;
use crate::typings::CustomLanguage;
use crate::values::{field, json_from_js, string_field, to_js};

thread_local! {
    /// Languages registered from JavaScript, by lowercase label.
    pub(crate) static REGISTERED: RefCell<HashMap<String, Language>> = RefCell::new(HashMap::new());
}

pub(crate) fn language_named(name: &str) -> Result<Language, JsError> {
    let key = name.trim().to_lowercase();
    if let Some(language) = REGISTERED.with(|languages| languages.borrow().get(&key).copied()) {
        return Ok(language);
    }
    // A registered language's file extension names it too, as `yml` does.
    let extension = key.trim_start_matches('.');
    if let Some(language) = REGISTERED.with(|languages| {
        languages.borrow().values().copied().find(|language| {
            language
                .extensions()
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
    }) {
        return Ok(language);
    }
    name.parse::<Language>()
        .map_err(|error| JsError::new(&error.to_string()))
}

/// `language` if documents can be written in it: an `editAs` target must.
pub(crate) fn writable_edit_as(language: Language) -> Result<Language, JsError> {
    language
        .serialize(&Value::Object(serde_json::Map::new()))
        .map(|_| language)
        .map_err(|error| JsError::new(&format!("editAs: {error}")))
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
];

/// A language implemented in JavaScript. The editor gives it what the shared
/// core provides from `parse` and the spans it locates: schema validation
/// with positions, conversion to and from every other language, Edit-as,
/// and the editing behavior its configuration describes. Schema completion
/// needs a language's own completion context, which only the built-in
/// languages have.
struct JsLanguage {
    definition: JsValue,
    info: LanguageInfo,
    config: LanguageConfig,
}

// SAFETY: wasm32 without the `atomics` target feature runs on one thread, so
// the JS object is never touched from another thread. The trait requires
// Send + Sync for hosts with threads.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for JsLanguage {}
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for JsLanguage {}

impl JsLanguage {
    fn method(&self, name: &str) -> Option<Function> {
        field(&self.definition, name).dyn_into::<Function>().ok()
    }

    fn span(&self, name: &str, source: &str, pointer: &str) -> Option<Span> {
        let span = self
            .method(name)?
            .call2(
                &self.definition,
                &JsValue::from_str(source),
                &JsValue::from_str(pointer),
            )
            .ok()?;
        let number = |key: &str| {
            field(&span, key)
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 1.0)
                .map(|value| value as usize)
        };
        // A position outside the text is no position (the problem is shown
        // unplaced); columns are clamped onto their line.
        let line = number("line")?;
        let text = source.split('\n').nth(line - 1)?;
        let width = text.chars().count() + 1;
        let start = number("startColumn")?.min(width);
        let end = number("endColumn")?.min(width).max(start);
        Some(Span {
            line,
            start_column: start,
            end_column: end,
        })
    }
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

#[cfg(not(target_feature = "atomics"))]
impl ConfigLanguage for JsLanguage {
    fn info(&self) -> LanguageInfo {
        self.info
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let parse = self.method("parse").ok_or_else(|| ParseError {
            message: "the language has no parse()".to_string(),
            line: None,
            column: None,
        })?;
        match parse.call1(&self.definition, &JsValue::from_str(source)) {
            Ok(value) if value.is_undefined() => Err(ParseError {
                message: "parse() returned nothing".to_string(),
                line: None,
                column: None,
            }),
            Ok(value) => json_from_js(&value, 0).map_err(|error| ParseError {
                message: format!("parse() returned a value that is not JSON: {error}"),
                line: None,
                column: None,
            }),
            Err(thrown) => {
                // A position outside the text is clamped onto it.
                let lines: Vec<&str> = source.split('\n').collect();
                let number = |key: &str| {
                    field(&thrown, key)
                        .as_f64()
                        .filter(|value| value.is_finite() && *value >= 1.0)
                        .map(|value| value as usize)
                };
                let line = number("line").map(|line| line.min(lines.len()));
                let column = number("column").map(|column| match line {
                    Some(line) => column.min(lines[line - 1].chars().count() + 1),
                    None => column,
                });
                Err(ParseError {
                    message: string_field(&thrown, "message")
                        .or_else(|| thrown.as_string())
                        .unwrap_or_else(|| "parse error".to_string()),
                    line,
                    column,
                })
            }
        }
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        let serialize = self
            .method("serialize")
            .ok_or_else(|| format!("{} cannot write documents", self.info.label))?;
        serialize
            .call1(&self.definition, &to_js(value))
            .map_err(|error| {
                string_field(&error, "message")
                    .or_else(|| error.as_string())
                    .unwrap_or_else(|| "serialize() failed".to_string())
            })?
            .as_string()
            .ok_or_else(|| "serialize() must return a string".to_string())
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        self.span("keySpan", source, pointer)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        self.span("valueSpan", source, pointer)
    }

    fn highlight(&self, line: &str) -> Vec<Token> {
        let Some(highlight) = self.method("highlight") else {
            return Vec::new();
        };
        let Ok(tokens) = highlight.call1(&self.definition, &JsValue::from_str(line)) else {
            return Vec::new();
        };
        if !Array::is_array(&tokens) {
            return Vec::new();
        }
        let mut out = Tokens::new();
        for token in Array::from(&tokens).iter() {
            let number = |key: &str| field(&token, key).as_f64().map(|value| value as usize);
            let kind = match string_field(&token, "kind").as_deref() {
                Some("key") => TokenKind::Key,
                Some("string") => TokenKind::String,
                Some("literal") => TokenKind::Literal,
                Some("comment") => TokenKind::Comment,
                Some("punctuation") => TokenKind::Punctuation,
                _ => TokenKind::Value,
            };
            // JavaScript string indices (UTF-16 units, as `indexOf` gives).
            let byte = |unit: usize| {
                let mut units = 0;
                for (index, character) in line.char_indices() {
                    if units >= unit {
                        return (units == unit).then_some(index);
                    }
                    units += character.len_utf16();
                }
                (units == unit).then_some(line.len())
            };
            if let (Some(start), Some(end)) = (number("start"), number("end"))
                && start < end
                && let (Some(start), Some(end)) = (byte(start), byte(end))
            {
                out.push(start, end, kind);
            }
        }
        out.finish()
    }

    fn lexical_state(&self, _source: &str, _cursor: usize) -> Lexical {
        Lexical::default()
    }

    fn config(&self) -> LanguageConfig {
        self.config
    }
}

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
            [open, close] => Some((leak(open.clone()), leak(close.clone()))),
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
            let single = |index: u32| {
                Array::is_array(&pair)
                    .then(|| Array::from(&pair).get(index).as_string())
                    .flatten()
                    .filter(|text| text.chars().count() == 1)
                    .and_then(|text| text.chars().next())
            };
            match (single(0), single(1)) {
                (Some(open), Some(close)) => brackets.push((open, close)),
                _ => {
                    return Err(JsError::new(
                        "brackets must be [open, close] pairs of single characters",
                    ));
                }
            }
        }
    }
    let leak_all = |items: Vec<String>| -> &'static [&'static str] {
        Box::leak(items.into_iter().map(leak).collect())
    };
    Ok(LanguageConfig {
        line_comments: leak_all(line_comments),
        block_comment,
        brackets: Box::leak(brackets.into_boxed_slice()),
        separators: leak_all(separators),
        quotes: &["\"", "'"],
    })
}

/// Registers a language implemented in JavaScript and returns its name for
/// `language` / `editAs` (its label, lowercased). Registering the same label
/// again replaces it for new editors.
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
    for name in ["serialize", "keySpan", "valueSpan", "highlight"] {
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
    let extensions: Vec<&'static str> =
        if extensions_value.is_undefined() || extensions_value.is_null() {
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
                    Ok(leak(extension))
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
                **extension == other.as_str() || language.extensions().contains(*extension)
            }) {
                return Err(JsError::new(&format!(
                    "extension \"{extension}\" already names the registered language \"{other}\""
                )));
            }
        }
        Ok(())
    })?;
    let config = language_config(&definition)?;
    let language: &'static JsLanguage = Box::leak(Box::new(JsLanguage {
        config,
        definition,
        info: LanguageInfo {
            label: leak(label),
            icon: leak(icon),
            extensions: Box::leak(extensions.into_boxed_slice()),
        },
    }));
    REGISTERED.with(|languages| {
        languages
            .borrow_mut()
            .insert(name.clone(), Language::new(language));
    });
    Ok(name)
}
