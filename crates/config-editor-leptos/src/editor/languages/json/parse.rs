//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;
use crate::editor::language::Said;
use crate::editor::text::{text_size, byte_range};

/// Parses with `serde_json` for syntax, then rejects duplicate keys in the same
/// object, which `serde_json` would otherwise silently keep the last of.
pub(super) fn json_parse(source: &str) -> Result<Value, ParseError> {
    let document: Value = serde_json::from_str(source).map_err(|error| {
        let line_count = source.split('\n').count().max(1);
        let line = error.line().clamp(1, line_count);
        // serde counts the column in bytes; the editor's are characters.
        let (mut line, mut column) = (line, super::super::byte_columns::char_column(source, line, error.column().max(1)));
        // The marker shows the position, so the message drops serde's
        // " at line N column M".
        let full = error.to_string();
        let message = full
            .rsplit_once(" at line ")
            .map_or(full.as_str(), |(text, _)| text)
            .to_string();
        let at = byte_at(source, line, column);
        // A comment (`// …`, `/* … */`, as some tools write in JSON files)
        // is reported as what it is, not as the token it stands in for.
        let message = if source[at..].starts_with("//") || source[at..].starts_with("/*") {
            "JSON has no comments: remove this one".to_string()
        } else if message == "recursion limit exceeded" {
            super::super::nesting::too_deep(super::super::nesting::VALUES)
        } else {
            super::super::json_words::plain_words(&error, source, "document")
        };
        // A trailing comma is reported at the closer; point at the comma.
        if full.starts_with("trailing comma")
            && let Some(comma) = source[..at].rfind(',')
        {
            (line, column) = line_column(source, text_size(comma));
        }
        // serde's own sentences quote nothing of the text.
        let (column, message) = super::super::wide_integers::out_of_range(source, line, column)
            .unwrap_or((column, Said::from(message)));
        message.at(Some(line), Some(column))
    })?;
    let written = |pointer: &str| {
        let (start, end) = *scanned(source).values.get(&pointer_segments(pointer))?;
        source.get(start..end).map(|text| (start, text.to_string()))
    };
    if let Some((_, bytes, text)) = super::super::wide_integers::first_past_64_bits(&document, &written) {
        let (line, column) = line_column(source, text_size(bytes.start));
        return Err(super::super::wide_integers::message(&text, bytes).at(Some(line), Some(column)));
    }
    let mut seen = HashSet::new();
    for (owner, key, start) in analyze_caret(source, 0).keys {
        if !seen.insert((owner, key.clone())) {
            let (line, column) = line_column(source, text_size(start));
            // The key as written, its quotes and escapes too.
            let written = start..crate::editor::languages::inline::skip_quoted(source, start);
            return Err(Said::from("duplicate key \"")
                .quote(&key, written)
                .words("\"")
                .at(Some(line), Some(column)));
        }
    }
    Ok(document)
}

/// The byte of `source` at 1-based `line` and character `column`.
fn byte_at(source: &str, line: usize, column: usize) -> usize {
    let line_start: usize = source.split_inclusive('\n').take(line - 1).map(str::len).sum();
    line_start
        + source[line_start..]
            .char_indices()
            .nth(column - 1)
            .map_or(source.len() - line_start, |(index, _)| index)
}

/// The spans of every key and value of a document, by path (the first
/// written, for a duplicate).
struct Spans {
    keys: std::collections::HashMap<Vec<String>, (usize, usize)>,
    values: std::collections::HashMap<Vec<String>, (usize, usize)>,
    /// Each value whole: a container from its opener through its closer.
    wholes: std::collections::HashMap<Vec<String>, (usize, usize)>,
}

/// The document's spans, read once for all the spans a batch of problems
/// asks for (see `paths::batch_cached`).
fn scanned(source: &str) -> std::rc::Rc<Spans> {
    crate::editor::paths::batch_cached(source, || {
        let scan = analyze(source, 0);
        let by_path = |spans: Vec<(Vec<String>, usize, usize)>| {
            let mut found = std::collections::HashMap::with_capacity(spans.len());
            for (path, start, end) in spans {
                found.entry(path).or_insert((start, end));
            }
            found
        };
        let wholes = scan
            .value_spans
            .iter()
            .zip(&scan.value_ends)
            .map(|((path, start, end), whole)| (path.clone(), *start, whole.unwrap_or(*end)))
            .collect();
        Spans { keys: by_path(scan.key_spans), values: by_path(scan.value_spans), wholes: by_path(wholes) }
    })
}

pub(super) fn json_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = *scanned(source).keys.get(&pointer_segments(path))?;
    Some(span_between(source, byte_range(start..end)))
}

/// The byte range of the value at `path` whole, a container through its
/// closer (`Text::value_ranges`).
pub(super) fn json_value_range(source: &str, path: &str) -> Option<std::ops::Range<usize>> {
    let (start, end) = *scanned(source).wholes.get(&pointer_segments(path))?;
    Some(start..end)
}

pub(super) fn json_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = *scanned(source).values.get(&pointer_segments(path))?;
    if source[start..].starts_with(['{', '[']) {
        // A container is marked at its opening bracket, not its whole body.
        let (line, column) = line_column(source, text_size(start));
        return Some((line, column, column + 1));
    }
    Some(span_between(source, byte_range(start..end)))
}

