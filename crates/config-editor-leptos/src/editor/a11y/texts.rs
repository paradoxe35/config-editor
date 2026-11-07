//! How keyboard and screen-reader access words problems, from the
//! `strings` setting.

use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use crate::EditorStrings;
use crate::strings::fill;

/// "Error", "Warning", "Info", "Hint".
pub(in crate::editor) fn severity_name(strings: &EditorStrings, severity: Severity) -> &str {
    match severity {
        Severity::Error => &strings.error,
        Severity::Warning => &strings.warning,
        Severity::Info => &strings.info,
        Severity::Hint => &strings.hint,
    }
}

/// " from burrow" for a host problem that names its source, else nothing.
fn source_part(strings: &EditorStrings, problem: &Diagnostic) -> String {
    problem
        .source
        .as_deref()
        .filter(|_| problem.origin == DiagnosticOrigin::Host)
        .map(|source| fill(&strings.problem_source, &[("source", &source)]))
        .unwrap_or_default()
}

/// "Error: …", "Warning from burrow: …": a problem as a screen reader
/// hears it.
pub(super) fn problem_text(strings: &EditorStrings, problem: &Diagnostic) -> String {
    fill(
        &strings.problem,
        &[
            ("severity", &severity_name(strings, problem.severity)),
            ("source", &source_part(strings, problem)),
            ("message", &problem.message),
        ],
    )
}

/// "Error 2 of 5: …": the problem F8 moved to, of the `total` placed ones.
pub(in crate::editor) fn problem_announcement(
    strings: &EditorStrings,
    problem: &Diagnostic,
    position: usize,
    total: usize,
) -> String {
    fill(
        &strings.problem_position,
        &[
            ("severity", &severity_name(strings, problem.severity)),
            ("position", &position),
            ("total", &total),
            ("source", &source_part(strings, problem)),
            ("message", &problem.message),
        ],
    )
}

/// "YAML Error on line 3: …": an underlined problem's accessible name.
pub(in crate::editor) fn problem_marker(
    strings: &EditorStrings,
    language: &str,
    problem: &Diagnostic,
    line: usize,
) -> String {
    fill(
        &strings.problem_marker,
        &[
            ("language", &language),
            ("severity", &severity_name(strings, problem.severity)),
            ("source", &source_part(strings, problem)),
            ("line", &line),
            ("message", &problem.message),
        ],
    )
}
