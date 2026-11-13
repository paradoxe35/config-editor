//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

/// Parses with `serde_json` for syntax, then rejects duplicate keys in the same
/// object, which `serde_json` would otherwise silently keep the last of.
pub(super) fn json_parse(source: &str) -> Result<Value, ParseError> {
    let document: Value = serde_json::from_str(source).map_err(|error| {
        let line_count = source.split('\n').count().max(1);
        let (mut line, mut column) = (error.line().clamp(1, line_count), error.column().max(1));
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
            message
        };
        // A trailing comma is reported at the closer; point at the comma.
        if message.starts_with("trailing comma")
            && let Some(comma) = source[..at].rfind(',')
        {
            (line, column) = line_column(source, comma);
        }
        ParseError {
            message: super::super::wide_integers::out_of_range(source, line, column).unwrap_or(message),
            line: Some(line),
            column: Some(column),
        }
    })?;
    let written = |pointer: &str| {
        let (start, end) = *scanned(source).values.get(&pointer_segments(pointer))?;
        source.get(start..end).map(str::to_string)
    };
    if let Some((pointer, text)) = super::super::wide_integers::first_past_64_bits(&document, &written) {
        let start = scanned(source).values.get(&pointer_segments(&pointer)).map(|(start, _)| *start);
        let (line, column) = start.map(|start| line_column(source, start)).unzip();
        return Err(ParseError {
            message: super::super::wide_integers::message(&text),
            line,
            column,
        });
    }
    let mut seen = HashSet::new();
    for (owner, key, start) in analyze_caret(source, 0).keys {
        if !seen.insert((owner, key.clone())) {
            let (line, column) = line_column(source, start);
            return Err(ParseError {
                message: format!("duplicate key \"{key}\""),
                line: Some(line),
                column: Some(column),
            });
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

pub(super) fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (line, column, column + source[start..end].chars().count())
}

/// The spans of every key and value of a document, by path (the first
/// written, for a duplicate).
struct Spans {
    keys: std::collections::HashMap<Vec<String>, (usize, usize)>,
    values: std::collections::HashMap<Vec<String>, (usize, usize)>,
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
        Spans { keys: by_path(scan.key_spans), values: by_path(scan.value_spans) }
    })
}

pub(super) fn json_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = *scanned(source).keys.get(&pointer_segments(path))?;
    Some(span_between(source, start, end))
}

pub(super) fn json_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = *scanned(source).values.get(&pointer_segments(path))?;
    if source[start..].starts_with(['{', '[']) {
        // A container is marked at its opening bracket, not its whole body.
        let (line, column) = line_column(source, start);
        return Some((line, column, column + 1));
    }
    Some(span_between(source, start, end))
}

pub(super) fn first_line_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}
