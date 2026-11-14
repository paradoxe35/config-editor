//! How folding words its controls and what it did, from the `strings`
//! setting.

use leptos::prelude::*;

use super::super::fold_ranges::FoldRange;
use super::super::state::EditorState;
use crate::strings::fill;

impl EditorState {
    /// Runs a fold command (a key, a chevron, a chip, the handle's `fold`
    /// and `fold_all`), then says what it folded or opened: the lines leave
    /// the text area or come back, which a screen reader does not see
    /// otherwise. Returns what the command did.
    pub(crate) fn fold_command<T>(&self, command: impl FnOnce() -> T) -> T {
        let before = self.fold().folded.get_untracked();
        let done = command();
        let said = self.fold().folded.with_untracked(|after| {
            self.strings_untracked(|strings| fold_change(strings, &before, after))
        });
        if let Some(said) = said {
            self.a11y().announce(said);
        }
        done
    }

    /// A fold key's command: as `fold_command`, and when it changed
    /// nothing (no range at the caret, the range already folded, nothing
    /// to open) it says `nothing` ("Nothing to fold") instead of staying
    /// silent.
    pub(crate) fn fold_key_command(
        &self,
        nothing: fn(&crate::EditorStrings) -> &String,
        command: impl FnOnce(),
    ) {
        let before = self.fold().folded.get_untracked();
        self.fold_command(command);
        if self.fold().folded.with_untracked(|after| *after == before) {
            let said = self.strings_untracked(|strings| nothing(strings).clone());
            self.a11y().announce(said);
        }
    }
}

/// What changing the closed folds from `before` to `after` did: "Folded
/// lines 3–7", "Unfolded lines 3–7", "Folded 4 ranges", …; `None` when
/// nothing changed.
pub(crate) fn fold_change(
    strings: &crate::EditorStrings,
    before: &[FoldRange],
    after: &[FoldRange],
) -> Option<String> {
    let closed: Vec<&FoldRange> = after.iter().filter(|fold| !before.contains(fold)).collect();
    let opened: Vec<&FoldRange> = before.iter().filter(|fold| !after.contains(fold)).collect();
    let lines = |template: &str, fold: &FoldRange| {
        fill(
            template,
            &[("first", &(fold.start + 1)), ("last", &(fold.end + 1))],
        )
    };
    let ranges = |template: &str, count: usize| fill(template, &[("count", &count)]);
    match (closed.as_slice(), opened.as_slice()) {
        ([], []) => None,
        ([fold], []) => Some(lines(&strings.folded_lines, fold)),
        ([], [fold]) => Some(lines(&strings.unfolded_lines, fold)),
        (closed, []) => Some(ranges(&strings.folded_ranges, closed.len())),
        ([], opened) => Some(ranges(&strings.unfolded_ranges, opened.len())),
        // No command folds some ranges and opens others.
        _ => None,
    }
}

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

#[cfg(test)]
mod change_tests {
    use super::*;

    fn range(start: usize, end: usize) -> FoldRange {
        FoldRange { start, end }
    }

    #[test]
    fn says_what_a_fold_command_did() {
        let strings = crate::EditorStrings::default();
        let said = |before: &[FoldRange], after: &[FoldRange]| fold_change(&strings, before, after);
        assert_eq!(said(&[], &[]), None);
        assert_eq!(
            said(&[], &[range(1, 3)]).as_deref(),
            Some("Folded lines 2–4")
        );
        assert_eq!(
            said(&[range(1, 3), range(5, 6)], &[range(5, 6)]).as_deref(),
            Some("Unfolded lines 2–4")
        );
        assert_eq!(
            said(&[], &[range(1, 3), range(5, 6)]).as_deref(),
            Some("Folded 2 ranges")
        );
        assert_eq!(
            said(&[range(1, 3), range(5, 6)], &[]).as_deref(),
            Some("Unfolded 2 ranges")
        );
        assert_eq!(said(&[range(1, 3)], &[range(1, 3)]), None);
    }
}
