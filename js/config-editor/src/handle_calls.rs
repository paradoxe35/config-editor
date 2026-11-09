//! What `ConfigEditor` and a document's `EditorView` answer through the
//! crate's [`EditorHandle`]: paths, ranges, folds, focus. One body for both
//! classes; each method of theirs is a call here. `None` (a document whose
//! editor has not mounted yet) answers `null`, `false` or nothing.

use config_editor_leptos::{EditorHandle, TextPosition, TextRange};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::typings::{LineRanges, PathResult, RangeResult};
use crate::values::{
    field, index_arg, line_ranges_to_js, path_from_js, path_to_js, range_out_to_js,
};

pub(crate) fn range_of(
    handle: Option<EditorHandle>,
    path: &JsValue,
) -> Result<RangeResult, JsError> {
    config_editor_leptos::js_support::flush_pending_input();
    let path = path_from_js(path)?;
    // The offsets read from what the editor keeps for its text: no copy of
    // the text, and no read of it up to the range, per call.
    let found = handle
        .and_then(|handle| config_editor_leptos::js_support::range_and_offsets_of(&handle, path));
    Ok(found
        .map_or(JsValue::NULL, |(range, start, end)| {
            range_out_to_js(range, start, end)
        })
        .unchecked_into())
}

pub(crate) fn path_at(
    handle: Option<EditorHandle>,
    offset: &JsValue,
) -> Result<PathResult, JsError> {
    config_editor_leptos::js_support::flush_pending_input();
    let offset = index_arg(offset, "offset")?;
    // From what the editor keeps for its text: no copy of the text, and no
    // read of it up to the offset, per call.
    let path =
        handle.and_then(|handle| config_editor_leptos::js_support::path_at_offset(&handle, offset));
    Ok(path
        .map_or(JsValue::NULL, |path| path_to_js(&path))
        .unchecked_into())
}

pub(crate) fn current_path(handle: Option<EditorHandle>) -> PathResult {
    untrack(|| handle.and_then(|handle| handle.current_path()))
        .map_or(JsValue::NULL, |path| path_to_js(&path))
        .unchecked_into()
}

pub(crate) fn text(handle: Option<EditorHandle>) -> Option<String> {
    config_editor_leptos::js_support::flush_pending_input();
    untrack(|| handle.and_then(|handle| handle.text()))
}

pub(crate) fn folded_lines(handle: Option<EditorHandle>) -> LineRanges {
    let folded = untrack(|| {
        handle
            .map(|handle| handle.folded_lines())
            .unwrap_or_default()
    });
    line_ranges_to_js(folded).unchecked_into()
}

pub(crate) fn reveal(handle: Option<EditorHandle>, path: &JsValue) -> Result<bool, JsError> {
    config_editor_leptos::js_support::flush_pending_input();
    let path = path_from_js(path)?;
    Ok(handle.is_some_and(|handle| handle.reveal(path)))
}

/// `revealRange({ start, end })`: string indices into the text on screen
/// (as `rangeOf` gives them), clamped to its end.
pub(crate) fn reveal_range(handle: Option<EditorHandle>, range: &JsValue) -> Result<bool, JsError> {
    config_editor_leptos::js_support::flush_pending_input();
    if !range.is_object() {
        return Err(JsError::new(
            "range must be { start, end } (string indices into the text on screen)",
        ));
    }
    let start = index_arg(&field(range, "start"), "start")?;
    let end = index_arg(&field(range, "end"), "end")?;
    let Some(handle) = handle else {
        return Ok(false);
    };
    let Some(text) = untrack(|| handle.text()) else {
        return Ok(false);
    };
    let range = TextRange::new(
        TextPosition::from_offset(&text, start),
        TextPosition::from_offset(&text, end),
    );
    Ok(handle.reveal_range(range))
}

pub(crate) fn fold(handle: Option<EditorHandle>, path: &JsValue) -> Result<bool, JsError> {
    config_editor_leptos::js_support::flush_pending_input();
    let path = path_from_js(path)?;
    Ok(handle.is_some_and(|handle| handle.fold(path)))
}

pub(crate) fn unfold(handle: Option<EditorHandle>, path: &JsValue) -> Result<bool, JsError> {
    config_editor_leptos::js_support::flush_pending_input();
    let path = path_from_js(path)?;
    Ok(handle.is_some_and(|handle| handle.unfold(path)))
}

pub(crate) fn fold_all(handle: Option<EditorHandle>) {
    config_editor_leptos::js_support::flush_pending_input();
    if let Some(handle) = handle {
        handle.fold_all();
    }
}

pub(crate) fn unfold_all(handle: Option<EditorHandle>) {
    if let Some(handle) = handle {
        handle.unfold_all();
    }
}

pub(crate) fn focus(handle: Option<EditorHandle>) {
    if let Some(handle) = handle {
        handle.focus();
    }
}
