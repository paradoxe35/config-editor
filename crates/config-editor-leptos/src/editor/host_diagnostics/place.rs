//! Placing the host's problems in the text on screen: a path where the
//! field is written, a range where it is (or, while editing as another
//! language, at the field it starts in).

use std::collections::HashSet;

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, DiagnosticOrigin};
use super::super::language::ConfigLanguage;
use super::super::paths::{TextRange, path_at, problem_span};
use super::super::work_count::{Work, note};
use super::{DiagnosticTarget, HostDiagnostic, is_web_url};
use crate::EditorStrings;
use crate::strings::fill;

/// The text a [`DiagnosticTarget::Range`] counts in.
#[derive(Clone, Copy)]
pub(in crate::editor) enum RangeText<'a> {
    /// The text on screen is the host's.
    OnScreen,
    /// Editing as another language: the host's text, in its language.
    Host(&'static dyn ConfigLanguage, &'a str),
}

/// Where a problem goes: `(line, column, end_line, end_column)`, with
/// `end_line` only when it ends on a later line.
type Position = (usize, usize, Option<usize>, usize);

/// `problems` as diagnostics on `source` (the text on screen, in `behavior`):
/// a path where the field is written, as schema problems are marked (the
/// value, or the key of a block); a range over the lines it covers when
/// the text on screen is the host's, else at the field it starts in (see
/// [`RangeText`]). What cannot be placed has no position and shows in the
/// footer, a field's problem with its path in front. Duplicates show once.
pub(in crate::editor) fn place(
    behavior: &dyn ConfigLanguage,
    source: &str,
    problems: &[HostDiagnostic],
    ranges: RangeText<'_>,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if problems.is_empty() {
        return Vec::new();
    }
    let by_field = |problem: &HostDiagnostic| match problem.target {
        DiagnosticTarget::Path(_) => true,
        DiagnosticTarget::Range(_) => matches!(ranges, RangeText::Host(..)),
        DiagnosticTarget::Document => false,
    };
    let document = problems
        .iter()
        .any(by_field)
        .then(|| behavior.parse(source).ok())
        .flatten();
    let at_field = |pointer: &str| -> Option<Position> {
        let written_as_block = match document
            .as_ref()
            .and_then(|document| document.pointer(pointer))
        {
            Some(Value::Object(map)) => !map.is_empty(),
            Some(Value::Array(items)) => !items.is_empty(),
            _ => false,
        };
        // A block is marked on its key (not on the brace opening it).
        written_as_block
            .then(|| behavior.key_span(source, pointer))
            .flatten()
            .or_else(|| problem_span(behavior, source, pointer, false, written_as_block))
            .map(|span| (span.line, span.start_column, None, span.end_column))
    };
    // The lines on screen, split once for every range (not per range: three
    // thousand ranges in a long document took a quarter of a second).
    let mut lines: Option<Vec<&str>> = None;
    let mut placed: Vec<Diagnostic> = Vec::new();
    let mut seen = HashSet::new();
    for problem in problems {
        let mut message = problem.message.clone();
        let position = match &problem.target {
            DiagnosticTarget::Path(path) if path.is_root() => None,
            DiagnosticTarget::Path(path) => {
                let pointer = path.to_pointer();
                let position = at_field(&pointer);
                if position.is_none() {
                    // Told apart from the same message at another field.
                    message = fill(
                        &strings.problem_at_path,
                        &[("path", &pointer), ("message", &message)],
                    );
                }
                position
            }
            DiagnosticTarget::Range(range) => match ranges {
                RangeText::OnScreen => covered(
                    lines.get_or_insert_with(|| {
                        note(Work::LineSplit);
                        source.split('\n').collect()
                    }),
                    *range,
                ),
                RangeText::Host(host, text) => {
                    through_field(behavior, source, (host, text), *range, &at_field)
                }
            },
            DiagnosticTarget::Document => None,
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
}

/// Where `range` is in the text of `lines`, kept inside it: from its start
/// to its end, over every line between. A range ending at the start of a
/// line ends at the end of the line before; one ending past the text, at
/// the end of its last line. `None` when it starts past the last line.
fn covered(lines: &[&str], range: TextRange) -> Option<Position> {
    // One past the last character of line `line` (one-based).
    let width = |line: usize| lines[line - 1].chars().count() + 1;
    let line = range.start.line.max(1);
    if line > lines.len() {
        return None;
    }
    let start = range.start.column.clamp(1, width(line));
    let (mut end_line, mut end) = if range.end.line > lines.len() {
        (lines.len(), usize::MAX)
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

/// Where `range` of the host's text goes in `source` (another language):
/// at the field it starts in, on its key when it starts on the key, else
/// where a problem with that field is marked. `None` when it starts in no
/// field (past the text, or at the top level).
fn through_field(
    behavior: &dyn ConfigLanguage,
    source: &str,
    (host, text): (&'static dyn ConfigLanguage, &str),
    range: TextRange,
    at_field: &dyn Fn(&str) -> Option<Position>,
) -> Option<Position> {
    let line_text = text.split('\n').nth(range.start.line.saturating_sub(1))?;
    // Starting in the line's indentation (a whole line selected from its
    // start), it starts at the field written on that line.
    let mut range = range;
    let indent = line_text
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    if range.start.column <= indent && indent < line_text.chars().count() {
        range.start.column = indent + 1;
    }
    let path = path_at(host, text, range.start.to_offset(text))?;
    if path.is_root() {
        return None;
    }
    let pointer = path.to_pointer();
    let on_key = host.key_span(text, &pointer).is_some_and(|key| {
        key.line == range.start.line
            && (key.start_column..key.end_column).contains(&range.start.column)
    });
    let key = on_key
        .then(|| behavior.key_span(source, &pointer))
        .flatten()
        .map(|span| (span.line, span.start_column, None, span.end_column));
    key.or_else(|| at_field(&pointer))
}
