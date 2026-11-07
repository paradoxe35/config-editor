//! Documents without an editor: parsing into plain data and writing it back,
//! where a field is written, which field an index is on, and what folds.
//! The same answers as an editor's `rangeOf` and `pathAt`.

use wasm_bindgen::prelude::*;

use crate::languages::language_named;
use crate::typings::{Data, LineRanges, PathResult, RangeResult};
use crate::values::{
    index_arg, json_from_js, json_to_js, line_ranges_to_js, path_at_index, path_from_js,
    path_to_js, range_to_js, text_arg,
};

/// Parses `text` into plain data (objects, arrays, strings, numbers,
/// booleans, `null`): the model schema validation sees, the same for every
/// language. Throws the first syntax error, with its line and column.
#[wasm_bindgen]
pub fn parse(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
) -> Result<Data, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let value = language
        .parse(&text_arg(&text, "text")?)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(json_to_js(&value).unchecked_into())
}

/// Writes plain data as a document in `language`. Throws when the value is
/// not JSON data or the language cannot express it (TOML has no `null`).
#[wasm_bindgen]
pub fn serialize(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "unknown")] value: JsValue,
) -> Result<String, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let value = json_from_js(&value, 0)
        .map_err(|error| JsError::new(&format!("value is not JSON data: {error}")))?;
    language
        .serialize(&value)
        .map_err(|error| JsError::new(&error.to_string()))
}

/// Where the field at `path` is written in `text`, or `null` when it has no
/// such field: the key and its value when they share a line, the key alone
/// when the value is a block below it, the value alone for a list item.
#[wasm_bindgen(js_name = rangeOf)]
pub fn range_of(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
) -> Result<RangeResult, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    let path = path_from_js(&path)?;
    Ok(language
        .range_of(&text, path)
        .map_or(JsValue::NULL, |range| range_to_js(&text, range))
        .unchecked_into())
}

/// The path of what `text` holds at string index `index`: the key under or
/// just before it, else the value or container holding it (`""` at the top
/// level); `null` past the end of the text.
#[wasm_bindgen(js_name = pathAt)]
pub fn path_at(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number")] index: JsValue,
) -> Result<PathResult, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    let index = index_arg(&index, "index")?;
    Ok(
        path_at_index(&text, index, |position| language.path_at(&text, position))
            .map_or(JsValue::NULL, |path| path_to_js(&path))
            .unchecked_into(),
    )
}

/// The blocks of `text` that fold, as the editor's gutter chevrons offer
/// them: one-based `{ startLine, endLine }`, `startLine` staying visible.
#[wasm_bindgen(js_name = foldRanges)]
pub fn fold_ranges(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
) -> Result<LineRanges, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let ranges = language.fold_ranges(&text_arg(&text, "text")?);
    Ok(
        line_ranges_to_js(ranges.iter().map(|range| (range.start + 1, range.end + 1)))
            .unchecked_into(),
    )
}
