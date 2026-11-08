//! How folding words its controls, from the `strings` setting.

/// The label of the chevron or chip of the range starting on zero-based
/// `line`: "Fold line 3" while open, "Unfold lines 3–7" when closed up to
/// zero-based `closed_end`.
pub(crate) fn fold_label(
    strings: &crate::EditorStrings,
    line: usize,
    closed_end: Option<usize>,
) -> String {
    match closed_end {
        Some(end) => crate::strings::fill(
            &strings.unfold,
            &[("first", &(line + 1)), ("last", &(end + 1))],
        ),
        None => crate::strings::fill(&strings.fold, &[("line", &(line + 1))]),
    }
}

/// The label of a closed fold's `…` chip: its [`fold_label`], then the most
/// serious problem it hides, if any ("Unfold lines 3–7, hides a problem:
/// Error"), so its colour is not the only sign of it.
pub(crate) fn chip_label(
    strings: &crate::EditorStrings,
    line: usize,
    closed_end: usize,
    hidden: Option<crate::Severity>,
) -> String {
    let label = fold_label(strings, line, Some(closed_end));
    match hidden {
        Some(severity) => {
            let name = super::super::a11y::severity_name(strings, severity);
            let problem = crate::strings::fill(&strings.folded_problem, &[("severity", &name)]);
            format!("{label}, {problem}")
        }
        None => label,
    }
}

/// What the `…` chip shows.
pub(crate) const CHIP: &str = "…";
