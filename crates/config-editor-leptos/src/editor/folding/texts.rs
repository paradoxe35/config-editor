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

/// The label of a closed fold's `…` chip: its [`fold_label`], then the
/// problems it hides, if any: the most serious and how many ("Unfold lines
/// 3–7, hides a problem: Error", "…, hides 2 problems, the most serious:
/// Error"), so its colour is not the only sign of them.
pub(crate) fn chip_label(
    strings: &crate::EditorStrings,
    line: usize,
    closed_end: usize,
    hidden: Option<(crate::Severity, usize)>,
) -> String {
    let label = fold_label(strings, line, Some(closed_end));
    match hidden {
        Some((severity, count)) => {
            let name = super::super::a11y::severity_name(strings, severity);
            let template = if count == 1 {
                &strings.folded_problem_one
            } else {
                &strings.folded_problem_other
            };
            let problem = crate::strings::fill(template, &[("count", &count), ("severity", &name)]);
            format!("{label}, {problem}")
        }
        None => label,
    }
}

/// What the `…` chip shows.
pub(crate) const CHIP: &str = "…";
