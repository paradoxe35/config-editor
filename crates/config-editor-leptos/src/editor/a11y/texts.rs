//! The built-in texts of keyboard and screen-reader access, in one place.

use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};

/// Announced when `Ctrl+M` turns Tab focus moving on.
pub(super) const TAB_FOCUS_ON: &str =
    "Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again.";

/// Announced when `Ctrl+M` turns it off again.
pub(super) const TAB_FOCUS_OFF: &str =
    "Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab.";

/// The footer's indicator while Tab moves focus.
pub(in crate::editor) const TAB_FOCUS_LABEL: &str = "Tab moves focus";

/// Its tooltip; clicking it turns the mode off.
pub(in crate::editor) const TAB_FOCUS_TITLE: &str = "Tab moves focus (Ctrl+M to indent with Tab)";

/// "Error", "Warning", "Info", "Hint".
fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Info => "Info",
        Severity::Hint => "Hint",
    }
}

/// " from burrow" for a host problem that names its source.
fn source_suffix(problem: &Diagnostic) -> String {
    problem
        .source
        .as_deref()
        .filter(|_| problem.origin == DiagnosticOrigin::Host)
        .map(|source| format!(" from {source}"))
        .unwrap_or_default()
}

/// "Error: …", "Warning from burrow: …": a problem as a screen reader
/// hears it.
pub(super) fn problem_text(problem: &Diagnostic) -> String {
    format!(
        "{}{}: {}",
        severity_name(problem.severity),
        source_suffix(problem),
        problem.message
    )
}

/// "Error 2 of 5: …": the problem F8 moved to, of the `total` placed ones.
pub(in crate::editor) fn problem_announcement(
    problem: &Diagnostic,
    position: usize,
    total: usize,
) -> String {
    format!(
        "{} {position} of {total}{}: {}",
        severity_name(problem.severity),
        source_suffix(problem),
        problem.message
    )
}
