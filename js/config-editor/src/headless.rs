//! Headless checks and comparisons: validation, conversion, diffs and
//! change counts, and schema warnings. Documents (`parse`, `rangeOf`, …)
//! are in `documents`, accepted names in `catalog`.

use config_editor_leptos::ChangeSummary;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::languages::language_named;
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

/// Syntax and schema problems in `text`, as the editor shows them. Without a
/// schema only syntax is checked.
#[wasm_bindgen]
pub fn validate(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string")] schema: JsValue,
) -> Result<Diagnostics, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    // Lines and columns in the document, as the editor gives them.
    let text = crate::text_lines::document(&text);
    let schema = optional_text(&schema, "schema")?;
    let problems = match schema {
        Some(json) => crate::schema_cache::compiled(&json)
            .map_err(|error| JsError::new(&error.to_string()))?
            .validate(language, text),
        None => config_editor_leptos::check_syntax(language, text),
    };
    Ok(diagnostics_to_js(problems).unchecked_into())
}

/// Converts `text` from one language to another. With a schema, lone XML
/// elements where it expects a list convert as one-item lists.
#[wasm_bindgen]
pub fn convert(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] from: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] to: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string")] schema: JsValue,
) -> Result<ConversionResult, JsError> {
    let schema = optional_text(&schema, "schema")?;
    let from = language_named(&text_arg(&from, "from")?)?;
    let to = language_named(&text_arg(&to, "to")?)?;
    let text = text_arg(&text, "text")?;
    // The document: a leading byte order mark is none of it.
    let text = crate::text_lines::document(&text);
    let converted = match schema {
        Some(json) => crate::schema_cache::compiled(&json)
            .map_err(|error| JsError::new(&error.to_string()))?
            .convert(from, text, to),
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

/// What the editor cannot use in a JSON Schema, one message each, such as a
/// `patternProperties` regex its engine cannot compile. An invalid schema
/// throws.
#[wasm_bindgen(js_name = schemaWarnings)]
pub fn schema_warnings(
    #[wasm_bindgen(unchecked_param_type = "string")] schema: JsValue,
) -> Result<Names, JsError> {
    let schema = crate::schema_cache::compiled(&text_arg(&schema, "schema")?)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(to_js(&schema.warnings()).unchecked_into())
}
