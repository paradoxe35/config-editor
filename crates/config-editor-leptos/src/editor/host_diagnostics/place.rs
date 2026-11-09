//! Placing the host's problems in the text on screen: a path where the
//! field is written, a range where it is (or, while editing as another
//! language, at the field it starts in).
//!
//! However many problems there are, each text is read once: the spans of
//! every field asked for are read together (`paths::with_spans`), and so
//! are the lines (thousands of problems, each reading a long document
//! again, took seconds after every edit).

use std::collections::HashSet;
use std::ops::Range;

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, DiagnosticOrigin};
use super::super::language::ConfigLanguage;
use super::super::paths::{
    Lines, TextPosition, TextRange, lines, memo_key_span, problem_span, with_spans,
};
use super::targets::{HostTargets, Target, targets};
use super::{DiagnosticTarget, HostDiagnostic, is_web_url};
use crate::EditorStrings;
use crate::strings::fill;

/// The text a [`DiagnosticTarget::Range`] counts in.
#[derive(Clone, Copy)]
pub(in crate::editor) enum RangeText<'a> {
    /// The text on screen is the host's.
    OnScreen,
    /// Editing as another language: the fields the ranges start in, in the
    /// host's text (found once for it, see [`HostTargets`]).
    Host(&'a HostTargets),
}

/// Where a problem goes: `(line, column, end_line, end_column)`, with
/// `end_line` only when it ends on a later line.
type Position = (usize, usize, Option<usize>, usize);

/// `problems` as diagnostics on `source` (the text on screen, in `behavior`):
/// a path where the field is written, as schema problems are marked (the
/// value, or the key of a block); a range over the lines it covers when
/// the text on screen is the host's, else at the field it starts in (see
/// [`RangeText`]); a point (a position with no end) on the secret
/// placeholder it is on (`placeholders`: byte ranges of the tokens of
/// `source` the host's pattern matches), else on one character. What
/// cannot be placed has no position and shows in the footer, a field's
/// problem with its path in front. Duplicates show once.
pub(in crate::editor) fn place(
    behavior: &dyn ConfigLanguage,
    source: &str,
    problems: &[HostDiagnostic],
    ranges: RangeText<'_>,
    placeholders: &[Range<usize>],
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if problems.is_empty() {
        return Vec::new();
    }
    let targets = targets(problems, ranges);
    let pointers: Vec<String> = targets
        .iter()
        .filter_map(|target| match target {
            Target::Field { pointer, .. } => Some(pointer.clone()),
            _ => None,
        })
        .collect();
    let document = (!pointers.is_empty())
        .then(|| behavior.parse(source).ok())
        .flatten();
    with_spans(behavior, source, &pointers, || {
        let lines = lines(source);
        let mut placed: Vec<Diagnostic> = Vec::new();
        let mut seen = HashSet::new();
        for (problem, target) in problems.iter().zip(targets) {
            let mut message = problem.message.clone();
            let position = match target {
                Target::Field { pointer, on_key } => {
                    let key = on_key
                        .then(|| memo_key_span(behavior, source, &pointer))
                        .flatten()
                        .map(|span| (span.line, span.start_column, None, span.end_column));
                    let position =
                        key.or_else(|| at_field(behavior, source, document.as_ref(), &pointer));
                    if position.is_none() && matches!(problem.target, DiagnosticTarget::Path(_)) {
                        // Told apart from the same message at another field.
                        message = fill(
                            &strings.problem_at_path,
                            &[("path", &pointer), ("message", &message)],
                        );
                    }
                    position
                }
                Target::Covered(range) => covered(source, &lines, range),
                Target::Point(position) => point(source, &lines, position, placeholders),
                Target::Nowhere => None,
            };
            let diagnostic = Diagnostic {
                line: position.map(|(line, ..)| line),
                column: position.map(|(_, column, ..)| column),
                end_line: position.and_then(|(_, _, end_line, _)| end_line),
                end_column: position.map(|(.., end)| end),
                message,
                severity: problem.severity,
                origin: DiagnosticOrigin::Host,
                code: problem.code.clone(),
                source: Some(problem.source.clone().unwrap_or_else(|| "host".to_string())),
                // A link opens only a web page (never `javascript:`).
                docs_url: problem.docs_url.clone().filter(|url| is_web_url(url)),
                ..Diagnostic::default()
            };
            if seen.insert(diagnostic.clone()) {
                placed.push(diagnostic);
            }
        }
        placed
    })
}

/// Where a problem with the field at `pointer` goes, as schema problems
/// are marked: the value, or the key of a block.
fn at_field(
    behavior: &dyn ConfigLanguage,
    source: &str,
    document: Option<&Value>,
    pointer: &str,
) -> Option<Position> {
    let written_as_block = match document.and_then(|document| document.pointer(pointer)) {
        Some(Value::Object(map)) => !map.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        _ => false,
    };
    // A block is marked on its key (not on the brace opening it).
    written_as_block
        .then(|| memo_key_span(behavior, source, pointer))
        .flatten()
        .or_else(|| problem_span(behavior, source, pointer, false, written_as_block))
        .map(|span| (span.line, span.start_column, None, span.end_column))
}

/// Where a point at `position` of `source` (`split` into its lines) goes:
/// on the whole of the placeholder it is on (one of `placeholders`, byte
/// ranges), else on one character: the one there, past a line's end the
/// place after it (an empty line's start). Nothing is read from what is
/// written around it. `None` past the last line.
fn point(
    source: &str,
    split: &Lines,
    position: TextPosition,
    placeholders: &[Range<usize>],
) -> Option<Position> {
    let line = position.line.max(1);
    if line > split.count() {
        return None;
    }
    let text = split.line(source, line - 1)?;
    let line_start = text.as_ptr() as usize - source.as_ptr() as usize;
    let line_end = line_start + text.len();
    let column = position.column.clamp(1, text.chars().count() + 1);
    let byte = line_start
        + text
            .char_indices()
            .nth(column - 1)
            .map_or(text.len(), |(at, _)| at);
    let column_of = |byte: usize| source[line_start..byte].chars().count() + 1;
    match placeholders
        .iter()
        .find(|token| token.start <= byte && byte < token.end)
    {
        Some(token) => Some((
            line,
            column_of(token.start.max(line_start)),
            None,
            column_of(token.end.min(line_end)),
        )),
        None => Some((line, column, None, column + 1)),
    }
}

/// Where `range` is in `source` (`split` into its lines), kept inside it: from its start to its
/// end, over every line between. A range ending at the start of a line
/// ends at the end of the line before; one ending past the text, at the
/// end of its last line. `None` when it starts past the last line.
fn covered(source: &str, split: &Lines, range: TextRange) -> Option<Position> {
    let lines = split.count();
    // One past the last character of line `line` (one-based).
    let width = |line: usize| {
        split
            .line(source, line - 1)
            .map_or(0, |text| text.chars().count())
            + 1
    };
    let line = range.start.line.max(1);
    if line > lines {
        return None;
    }
    let start = range.start.column.clamp(1, width(line));
    let (mut end_line, mut end) = if range.end.line > lines {
        (lines, usize::MAX)
    } else {
        (range.end.line.max(line), range.end.column)
    };
    // Nothing of the end line covered (or the empty line after a final
    // line break): the range ends at the end of the line before.
    while end_line > line && (end <= 1 || (end == usize::MAX && width(end_line) == 1)) {
        end_line -= 1;
        end = usize::MAX;
    }
    end = end.clamp(1, width(end_line));
    if end_line == line {
        // An empty range still marks one character.
        return Some((line, start, None, end.max(start + 1)));
    }
    Some((line, start, Some(end_line), end))
}
