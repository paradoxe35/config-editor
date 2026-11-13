//! Headless checks and comparisons: validation, conversion, diffs and
//! change counts, and schema warnings. Documents (`parse`, `rangeOf`, …)
//! are in `documents`, accepted names in `catalog`.

use config_editor_leptos::{ChangeSummary, Schema};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::languages::language_named;
use crate::options::warn_unknown_keys;
use crate::schema_formats::{SOURCE_KEYS, SchemaSpec};
use crate::typings::{ChangeSummaryResult, ConversionResult, Diagnostics, Names};
use crate::values::{
    ChangeSummaryOut, context_arg, diagnostics_to_js, optional_text, text_arg, to_js,
};

/// The unified diff (`diff -u`) of `baseline` against `current`, with
/// `context` unchanged lines around each change (default 3): `--- baseline`
/// and `+++ current` headers, then `@@` hunks. Empty when they are the same.
#[wasm_bindgen(js_name = unifiedDiff)]
pub fn unified_diff(
    #[wasm_bindgen(unchecked_param_type = "string")] baseline: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] current: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "number")] context: JsValue,
) -> Result<String, JsError> {
    Ok(config_editor_leptos::unified_diff(
        &text_arg(&baseline, "baseline")?,
        &text_arg(&current, "current")?,
        context_arg(&context)?,
    ))
}

/// How `current` differs from `baseline`, by lines, as the diff gutter and
/// `onChangeSummary` count it (line endings ignored). A final line break
/// added or removed is the empty last line added or removed, where
/// `unifiedDiff` writes it as `diff -u` does (the last line changed).
#[wasm_bindgen(js_name = changeSummary)]
pub fn change_summary(
    #[wasm_bindgen(unchecked_param_type = "string")] baseline: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] current: JsValue,
) -> Result<ChangeSummaryResult, JsError> {
    let summary = ChangeSummary::between(
        &text_arg(&baseline, "baseline")?,
        &text_arg(&current, "current")?,
    );
    Ok(to_js(&ChangeSummaryOut::from(summary)).unchecked_into())
}

/// The schema `text` in the format `source` gives (JSON Schema by default),
/// compiled once for many calls (see `schema_cache`).
fn schema_from(text: &str, source: &JsValue) -> Result<Schema, JsError> {
    if !source.is_undefined() && !source.is_null() {
        warn_unknown_keys(
            source,
            &[
                SOURCE_KEYS.format,
                SOURCE_KEYS.resources,
                SOURCE_KEYS.options,
            ],
            "option",
        );
    }
    let spec = SchemaSpec::read(source, &SOURCE_KEYS, "")?;
    crate::schema_cache::compiled(text, &spec).map_err(|error| JsError::new(&error.to_string()))
}

/// Syntax and schema problems in `text`, as the editor shows them. Without a
/// schema only syntax is checked. A schema in another format than JSON
/// Schema names it in `source`.
#[wasm_bindgen]
pub fn validate(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string")] schema: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SchemaSourceOptions")] source: JsValue,
) -> Result<Diagnostics, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    let text = text.as_str();
    let schema = optional_text(&schema, "schema")?;
    let problems = match schema {
        Some(schema) => schema_from(&schema, &source)?.validate(language, text),
        None => config_editor_leptos::check_syntax(language, text),
    };
    Ok(diagnostics_to_js(problems).unchecked_into())
}

/// Converts `text` from one language to another. With a schema, lone XML
/// elements where it expects a list convert as one-item lists. A schema in
/// another format than JSON Schema names it in `source`.
#[wasm_bindgen]
pub fn convert(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] from: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] to: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string")] schema: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SchemaSourceOptions")] source: JsValue,
) -> Result<ConversionResult, JsError> {
    let schema = optional_text(&schema, "schema")?;
    let from = language_named(&text_arg(&from, "from")?)?;
    let to = language_named(&text_arg(&to, "to")?)?;
    let text = text_arg(&text, "text")?;
    let text = text.as_str();
    let converted = match schema {
        Some(schema) => schema_from(&schema, &source)?.convert(from, text, to),
        None => from.convert(text, to),
    }
    .map_err(|error| JsError::new(&error.to_string()))?;
    #[derive(Serialize)]
    struct Out {
        text: String,
        losses: Vec<String>,
    }
    Ok(to_js(&Out {
        text: converted.text,
        losses: converted.losses,
    })
    .unchecked_into())
}

/// What the editor cannot use in a schema, one message each, such as a
/// `patternProperties` regex its engine cannot compile, then what its format
/// loses (`compileSchema` gives those structured). An invalid schema
/// throws.
#[wasm_bindgen(js_name = schemaWarnings)]
pub fn schema_warnings(
    #[wasm_bindgen(unchecked_param_type = "string")] schema: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SchemaSourceOptions")] source: JsValue,
) -> Result<Names, JsError> {
    let schema = schema_from(&text_arg(&schema, "schema")?, &source)?;
    Ok(to_js(&schema.warnings()).unchecked_into())
}
