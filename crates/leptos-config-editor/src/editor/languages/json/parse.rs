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
        // A trailing comma is reported at the closer; point at the comma.
        if message.starts_with("trailing comma") {
            let line_start: usize = source.split_inclusive('\n').take(line - 1).map(str::len).sum();
            let at = line_start
                + source[line_start..]
                    .char_indices()
                    .nth(column - 1)
                    .map_or(source.len() - line_start, |(index, _)| index);
            if let Some(comma) = source[..at.min(source.len())].rfind(',') {
                (line, column) = line_column(source, comma);
            }
        }
        ParseError {
            message,
            line: Some(line),
            column: Some(column),
        }
    })?;
    let mut seen = HashSet::new();
    for (owner, key, start) in analyze(source, 0).keys {
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

pub(super) fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (line, column, column + source[start..end].chars().count())
}

pub(super) fn json_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    analyze(source, 0)
        .key_spans
        .into_iter()
        .find(|(segments, _, _)| *segments == wanted)
        .map(|(_, start, end)| span_between(source, start, end))
}

pub(super) fn json_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let (_, start, end) = analyze(source, 0)
        .value_spans
        .into_iter()
        .find(|(segments, _, _)| *segments == wanted)?;
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
