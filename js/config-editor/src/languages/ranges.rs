//! Strings and comments a JavaScript language writes over several lines:
//! its `multilineRanges(text)` callback, read into the [`ConfigLanguage`]
//! hooks that color them (`comments`, `multiline_strings`), keep their inner
//! lines text (`string_body_lines`) and keep suggestions out of them
//! (`lexical_state`).

use std::ops::Range;

use config_editor_leptos::language::{ConfigLanguage, utf16_to_byte_index};
use js_sys::Array;
use wasm_bindgen::prelude::*;

use super::js_language::JsLanguage;
use crate::values::field;

/// The byte ranges `multilineRanges` gave, each list in order.
#[derive(Clone, Default)]
pub(super) struct Multiline {
    pub(super) strings: Vec<Range<usize>>,
    pub(super) comments: Vec<Range<usize>>,
}

const SHAPE: &str = "{ start: number, end: number, kind: \"string\" | \"comment\" }[]";

/// Calls `multilineRanges(text)`; an item of the wrong shape is warned about
/// and left out.
pub(super) fn multiline_ranges(language: &JsLanguage, source: &str) -> Multiline {
    let mut out = Multiline::default();
    let Some(result) = language.call("multilineRanges", &[JsValue::from_str(source)]) else {
        return out;
    };
    if result.is_null() || result.is_undefined() {
        return out;
    }
    if !Array::is_array(&result) {
        language.warn("multilineRanges", &format!("must return {SHAPE}"));
        return out;
    }
    let length = source.encode_utf16().count();
    let index = |item: &JsValue, name: &str| {
        field(item, name)
            .as_f64()
            .filter(|number| number.is_finite() && number.fract() == 0.0 && *number >= 0.0)
            .map(|number| number as usize)
            .filter(|number| *number <= length)
    };
    for item in Array::from(&result).iter() {
        let start = index(&item, "start");
        let end = index(&item, "end");
        let kind = field(&item, "kind").as_string();
        let range = match (start, end) {
            (Some(start), Some(end)) if start <= end => {
                Some(utf16_to_byte_index(source, start)..utf16_to_byte_index(source, end))
            }
            _ => None,
        };
        match (range, kind.as_deref()) {
            (Some(range), Some("string")) => out.strings.push(range),
            (Some(range), Some("comment")) => out.comments.push(range),
            _ => language.warn(
                "multilineRanges",
                &format!("returned an item that is not one of {SHAPE} within the text"),
            ),
        }
    }
    out.strings.sort_by_key(|range| range.start);
    out.comments.sort_by_key(|range| range.start);
    out
}

/// Whether UTF-16 `cursor` is inside one of the comments (past its start;
/// at its end only when it runs to the end of the text, unclosed).
pub(super) fn in_comment(ranges: &Multiline, source: &str, cursor: usize) -> bool {
    let byte = utf16_to_byte_index(source, cursor);
    ranges
        .comments
        .iter()
        .any(|range| range.start < byte && (byte < range.end || range.end == source.len()))
}

/// [`ConfigLanguage::comments`]: the line comments outside the ranges, and
/// the comments among them, in order.
pub(super) fn comments(
    language: &JsLanguage,
    ranges: &Multiline,
    source: &str,
) -> Vec<Range<usize>> {
    let inside = |at: usize| {
        ranges
            .strings
            .iter()
            .chain(&ranges.comments)
            .any(|range| range.start <= at && at < range.end)
    };
    let mut comments = ranges.comments.clone();
    let mut line_start = 0;
    for line in source.split('\n') {
        if let Some(start) = language.comment_start(line)
            && !inside(line_start + start)
        {
            comments.push(line_start + start..line_start + line.len());
        }
        line_start += line.len() + 1;
    }
    comments.sort_by_key(|range| range.start);
    comments
}

/// [`ConfigLanguage::string_body_lines`]: the lines wholly inside each
/// string, past its first line and before its last.
pub(super) fn body_lines(source: &str, strings: &[Range<usize>]) -> Vec<Range<usize>> {
    let line_of = |byte: usize| source[..byte].matches('\n').count();
    strings
        .iter()
        .filter_map(|range| {
            let (first, last) = (line_of(range.start), line_of(range.end));
            (last > first + 1).then(|| first + 1..last)
        })
        .collect()
}
