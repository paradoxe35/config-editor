//! The checks a schema format written in JavaScript returns beside its
//! JSON Schema (`compile()`'s `checks`): each runs where the editor
//! validates, as the built-in formats' checks do.

use std::borrow::Cow;

use config_editor_leptos::{CheckProblem, CheckedDocument, SchemaCheck, Severity};
use js_sys::{Array, Function, Object};
use wasm_bindgen::prelude::*;

use crate::values::{field, json_to_js, string_field};

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** A rule of a schema format that JSON Schema cannot hold, checked where the editor validates (each time it validates, on the same text). Returned by a `CustomSchemaFormat`'s `compile()` in `checks`. */
export interface SchemaFormatCheck {
  /** What it checks, in words. */
  name?: string;
  /** The problems of `document`, each at the JSON pointer of the value it is about. A check that throws is shown as a warning at the top. */
  check(document: CheckedDocument): CheckProblemInput[];
}

/** The document a `SchemaFormatCheck` checks. */
export interface CheckedDocument {
  /** The text being validated. */
  text: string;
  /** Its language's label, such as `"YAML"`. */
  language: string;
  /** Its value, as read by the schema. */
  value: unknown;
  /** The compiled JSON Schema, with any annotations the format added for its checks. */
  schema: object;
}

/** A problem a `SchemaFormatCheck` found. The pointer is shown before the message, as before a schema problem. */
export interface CheckProblemInput {
  /** The JSON pointer of the value (`""`: the document). */
  pointer: string;
  message: string;
  /** Marked on the key rather than the value. */
  atKey?: boolean;
  /** Default `"error"`. */
  severity?: "error" | "warning" | "info";
}
"#;

/// One check from JavaScript: the object and its `check` method.
pub(crate) struct JsCheck {
    object: JsValue,
    check: Function,
    name: String,
    /// The format's label, for a check that throws.
    format: &'static str,
}

// SAFETY: wasm32 without the `atomics` target feature runs on one thread, so
// the JS objects are never touched from another thread.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for JsCheck {}
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for JsCheck {}

/// The checks `value` (`compile()`'s `checks`) holds.
pub(crate) fn checks_from(
    value: &JsValue,
    format: &'static str,
) -> Result<Vec<std::sync::Arc<dyn SchemaCheck>>, String> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(value) {
        return Err("must be an array".to_string());
    }
    Array::from(value)
        .iter()
        .enumerate()
        .map(|(index, object)| {
            let check = field(&object, "check")
                .dyn_into::<Function>()
                .map_err(|_| format!("[{index}] needs check(document)"))?;
            let name = string_field(&object, "name").unwrap_or_else(|| format!("check {index}"));
            Ok(std::sync::Arc::new(JsCheck {
                object,
                check,
                name,
                format,
            }) as std::sync::Arc<dyn SchemaCheck>)
        })
        .collect()
}

impl SchemaCheck for JsCheck {
    fn name(&self) -> Cow<'static, str> {
        Cow::Owned(self.name.clone())
    }

    fn check(&self, document: &CheckedDocument<'_>) -> Vec<CheckProblem> {
        let argument = Object::new();
        let set = |key: &str, value: &JsValue| {
            let _ = js_sys::Reflect::set(&argument, &JsValue::from_str(key), value);
        };
        set("text", &JsValue::from_str(document.text));
        set(
            "language",
            &JsValue::from_str(document.language.info().label),
        );
        set("value", &json_to_js(document.value));
        set("schema", &json_to_js(document.schema));
        let threw = |message: String| {
            vec![
                CheckProblem::new(
                    "",
                    format!(
                        "{}'s check \"{}\" did not run: {message}",
                        self.format, self.name
                    ),
                )
                .severity(Severity::Warning),
            ]
        };
        let result = match self.check.call1(&self.object, &argument) {
            Ok(result) => result,
            Err(error) => {
                let message = string_field(&error, "message")
                    .or_else(|| error.as_string())
                    .unwrap_or_else(|| "it threw".to_string());
                return threw(message);
            }
        };
        if result.is_undefined() || result.is_null() {
            return Vec::new();
        }
        if !Array::is_array(&result) {
            return threw("it must return an array of problems".to_string());
        }
        let mut problems = Vec::new();
        for (index, problem) in Array::from(&result).iter().enumerate() {
            let (Some(pointer), Some(message)) = (
                string_field(&problem, "pointer"),
                string_field(&problem, "message"),
            ) else {
                return threw(format!("problem [{index}] needs a pointer and a message"));
            };
            let mut found = CheckProblem::new(pointer, message);
            if field(&problem, "atKey").as_bool() == Some(true) {
                found = found.at_key();
            }
            found = match string_field(&problem, "severity").as_deref() {
                None | Some("error") => found,
                Some("warning") => found.severity(Severity::Warning),
                Some("info") => found.severity(Severity::Info),
                Some(other) => {
                    return threw(format!(
                        "problem [{index}] has severity \"{other}\" (expected error, warning or info)"
                    ));
                }
            };
            problems.push(found);
        }
        problems
    }
}
