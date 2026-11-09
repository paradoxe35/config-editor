//! Host ranges: kept over every line they cover on the host's text, and
//! placed at the field they start in while editing as another language.
//! Each test lists every failing case at once.

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::paths::{TextPosition, TextRange};
use super::tests::{DOCUMENTS, marked, report};
use super::{HostDiagnostic, HostTargets, RangeText, place};
use crate::strings::default_strings;

fn range(line: usize, column: usize, end_line: usize, end_column: usize) -> TextRange {
    TextRange::new(
        TextPosition::new(line, column),
        TextPosition::new(end_line, end_column),
    )
}

/// `(line, column, end_line, end_column)` of a placed problem.
fn position(diagnostic: &Diagnostic) -> Option<(usize, usize, Option<usize>, usize)> {
    Some((
        diagnostic.line?,
        diagnostic.column?,
        diagnostic.end_line,
        diagnostic.end_column?,
    ))
}

fn placed_on_screen(language: Language, text: &str, range: TextRange) -> Vec<Diagnostic> {
    let problems = [HostDiagnostic::at_range(range, Severity::Error, "x")];
    place(
        language.behavior(),
        text,
        &problems,
        RangeText::OnScreen,
        default_strings(),
    )
}

#[test]
fn a_range_over_several_lines_keeps_its_end_line() {
    let text = "listen: 7835\nweb:\n  http: 80\n";
    let cases = [
        // Three lines: from its start column to its end column.
        (range(1, 9, 3, 5), Some((1, 9, Some(3), 5))),
        // Two lines.
        (range(2, 1, 3, 5), Some((2, 1, Some(3), 5))),
        // Ending at the start of a line: up to the end of the one before.
        (range(1, 1, 2, 1), Some((1, 1, None, 13))),
        (range(1, 9, 3, 1), Some((1, 9, Some(2), 5))),
        // Past the end of the text: up to the end of its last line.
        (range(2, 1, 99, 1), Some((2, 1, Some(3), 11))),
        (range(2, 1, 99, 40), Some((2, 1, Some(3), 11))),
        // One line, as before.
        (range(1, 9, 1, 13), Some((1, 9, None, 13))),
        (range(9, 1, 12, 2), None),
    ];
    let mut failures = Vec::new();
    for (range, expected) in cases {
        let placed = placed_on_screen(Language::YAML, text, range);
        let found = placed.first().and_then(position);
        if placed.len() != 1 || found != expected {
            failures.push(format!("{range:?}: expected {expected:?}, found {found:?}"));
        }
    }
    report(failures);
}

#[test]
fn a_range_over_several_lines_is_kept_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in DOCUMENTS {
        let placed = placed_on_screen(language, text, range(2, 1, 4, 3));
        let found = placed.first().and_then(position);
        if found != Some((2, 1, Some(4), 3)) {
            failures.push(format!("{language}: {found:?}"));
        }
    }
    report(failures);
}

/// The range covering the first `needle` of `text`.
fn range_of(text: &str, needle: &str) -> TextRange {
    let at = text.find(needle).expect("needle in the text");
    let start = TextPosition::from_offset(text, text[..at].encode_utf16().count());
    let end = TextPosition::from_offset(text, text[..at + needle.len()].encode_utf16().count());
    TextRange::new(start, end)
}

/// QA 19 (H19-F9): a range starting in a line's indentation (a whole line
/// selected from its start) goes to the field written on that line, as one
/// starting at its first character does, not to the field around it.
#[test]
fn a_range_from_a_lines_indentation_is_placed_at_the_field_on_that_line() {
    let mut failures = Vec::new();
    for (host, host_text) in DOCUMENTS {
        let line = host_text
            .split('\n')
            .position(|text| text.contains("http"))
            .expect("the http line")
            + 1;
        let text = host_text.split('\n').nth(line - 1).unwrap();
        let indent = text.len() - text.trim_start().len();
        if indent == 0 {
            continue;
        }
        let end = text.chars().count() + 1;
        for (edit, shown) in DOCUMENTS.into_iter().filter(|(edit, _)| *edit != host) {
            let placed_from = |column: usize| {
                let problems = [HostDiagnostic::at_range(
                    range(line, column, line, end),
                    Severity::Error,
                    "x",
                )];
                let found = HostTargets::new(host.behavior(), host_text, &problems);
                let ranges = RangeText::Host(&found);
                place(edit.behavior(), shown, &problems, ranges, default_strings())
                    .first()
                    .and_then(|problem| marked(shown, problem))
            };
            let (from_start, from_text) = (placed_from(1), placed_from(indent + 1));
            if from_start != from_text || from_text.is_none() {
                failures.push(format!(
                    "{host} as {edit}: from the indentation {from_start:?}, from the text {from_text:?}"
                ));
            }
        }
    }
    report(failures);
}

#[test]
fn a_range_is_placed_at_its_field_while_editing_as_every_other_language() {
    // (what the host's range covers, what is underlined on screen)
    let key = |language: Language| {
        if language == Language::JSON {
            "\"http\""
        } else {
            "http"
        }
    };
    let mut failures = Vec::new();
    for (host, host_text) in DOCUMENTS {
        for (edit, shown) in DOCUMENTS.into_iter().filter(|(edit, _)| *edit != host) {
            let cases = [
                ("value", range_of(host_text, "80"), Some("80".to_string())),
                (
                    "key",
                    range_of(host_text, key(host)),
                    Some(key(edit).to_string()),
                ),
                // Past the host's text: no field, unplaced as before.
                ("past the end", range(40, 1, 40, 2), None),
            ];
            for (name, range, expected) in cases {
                let problems = [HostDiagnostic::at_range(range, Severity::Error, "x")];
                let found = HostTargets::new(host.behavior(), host_text, &problems);
                let ranges = RangeText::Host(&found);
                let placed = place(edit.behavior(), shown, &problems, ranges, default_strings());
                let found = placed.first().and_then(|problem| marked(shown, problem));
                if placed.len() != 1 || found != expected {
                    failures.push(format!(
                        "{host} as {edit}, range on the {name}: expected {expected:?}, found {found:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}
