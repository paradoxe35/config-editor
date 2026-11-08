//! The [`ConfigLanguage`] a `registerLanguage` definition is adapted into.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Range;

use config_editor_leptos::language::{
    CompletionContext, ConfigLanguage, InsertContext, LanguageConfig, LanguageInfo, Lexical,
    ParseError, SchemaPathSegment, Span, Token, TokenKind, Tokens,
};
use js_sys::{Array, Function};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use super::assist;
use super::ranges::{self, Multiline};
use crate::values::{field, json_from_js, string_field, to_js};

/// A language implemented in JavaScript. The editor gives it what the shared
/// core provides from `parse` and the spans it locates: schema validation
/// with positions, conversion to and from every other language, Edit-as,
/// the editing behavior its configuration describes, and schema completion
/// and hover: through its optional `contextAt`, `insertKey` and `keyAt`
/// callbacks, each one in place of its part of the flat default that its
/// `separators` give (one `KEY=value` entry per line; see
/// [`LanguageConfig::flat_completion_context`]). A flat language without
/// `keySpan` / `valueSpan` has its fields found on their lines; any other
/// has its read-only fields checked by value alone. Constructs written over
/// lines come from `multilineRanges`.
pub(super) struct JsLanguage {
    pub(super) definition: JsValue,
    pub(super) info: LanguageInfo,
    pub(super) config: LanguageConfig,
    /// `multilineRanges` of the last text it was called for.
    pub(super) multiline: RefCell<Option<(String, Multiline)>>,
}

// SAFETY: wasm32 without the `atomics` target feature runs on one thread, so
// the JS object is never touched from another thread. The trait requires
// Send + Sync for hosts with threads.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for JsLanguage {}
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for JsLanguage {}

impl JsLanguage {
    pub(super) fn method(&self, name: &str) -> Option<Function> {
        field(&self.definition, name).dyn_into::<Function>().ok()
    }

    /// Calls the definition's `name` with `arguments`: `None` when it has no
    /// such method or it threw (logged, as a throwing host callback is).
    pub(super) fn call(&self, name: &str, arguments: &[JsValue]) -> Option<JsValue> {
        let method = self.method(name)?;
        let result = match arguments {
            [first, second] => method.call2(&self.definition, first, second),
            [first] => method.call1(&self.definition, first),
            _ => method.call0(&self.definition),
        };
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                web_sys::console::error_2(
                    &JsValue::from_str(&format!(
                        "config-editor: {}'s {name}() threw",
                        self.info.label
                    )),
                    &error,
                );
                None
            }
        }
    }

    /// A console warning that `callback` returned something it may not
    /// (`problem`), once per language, callback and problem: the result is
    /// read as `null`.
    pub(super) fn warn(&self, callback: &str, problem: &str) {
        thread_local! {
            static WARNED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
        }
        let message = format!(
            "config-editor: {}'s {callback}() {problem}; read as null",
            self.info.label
        );
        if WARNED.with(|warned| warned.borrow_mut().insert(message.clone())) {
            web_sys::console::warn_1(&JsValue::from_str(&message));
        }
    }

    /// Whether the definition has the method `name`.
    pub(super) fn has(&self, name: &str) -> bool {
        self.method(name).is_some()
    }

    /// Whether the language is flat (`separators` and no `contextAt`: one
    /// `KEY=value` entry per line), so its fields are found on their lines
    /// when it gives no `keySpan` / `valueSpan`.
    fn is_flat(&self) -> bool {
        !self.config.separators.is_empty() && !self.has("contextAt")
    }

    /// The `multilineRanges` of `source`, called once per text.
    fn multiline(&self, source: &str) -> Multiline {
        if !self.has("multilineRanges") {
            return Multiline::default();
        }
        if let Some((text, ranges)) = self.multiline.borrow().as_ref()
            && text == source
        {
            return ranges.clone();
        }
        let ranges = ranges::multiline_ranges(self, source);
        *self.multiline.borrow_mut() = Some((source.to_string(), ranges.clone()));
        ranges
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
        if self.has("keySpan") {
            self.span("keySpan", source, pointer)
        } else if self.is_flat() {
            self.config.flat_key_span(source, pointer)
        } else {
            None
        }
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        if self.has("valueSpan") {
            self.span("valueSpan", source, pointer)
        } else if self.is_flat() {
            self.config.flat_value_span(source, pointer)
        } else {
            None
        }
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

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        Lexical {
            in_comment: ranges::in_comment(&self.multiline(source), source, cursor),
            ..Lexical::default()
        }
    }

    fn comments(&self, source: &str) -> Vec<Range<usize>> {
        ranges::comments(self, &self.multiline(source), source)
    }

    fn multiline_strings(&self, source: &str) -> Vec<Range<usize>> {
        self.multiline(source).strings
    }

    fn string_body_lines(&self, source: &str) -> Vec<Range<usize>> {
        ranges::body_lines(source, &self.multiline(source).strings)
    }

    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        if self.has("contextAt") {
            assist::context_at(self, source, cursor)
        } else {
            self.config.flat_completion_context(source, cursor)
        }
    }

    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        if self.has("contextAt") {
            assist::parsed_keys(self, source, cursor_byte, path)
        } else {
            self.config.flat_existing_keys(source, cursor_byte, path)
        }
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        if self.has("insertKey") {
            assist::insert_key(self, key, property, root, context)
        } else {
            self.config.flat_property_insert(key)
        }
    }

    fn format_value(&self, value: &Value) -> String {
        // A nested language (one that places the caret itself) writes
        // values as JSON, which such formats read; a flat one bare.
        if self.has("contextAt") {
            value.to_string()
        } else {
            self.config.flat_format_value(value)
        }
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        if self.has("keyAt") {
            return None;
        }
        self.config.flat_hover_key(line, column)
    }

    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        if !self.has("keyAt") {
            return None;
        }
        assist::key_at(self, source, line_index, column)
    }

    fn config(&self) -> LanguageConfig {
        self.config
    }
}
