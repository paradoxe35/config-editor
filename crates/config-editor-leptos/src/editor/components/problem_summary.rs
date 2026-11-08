//! The footer's problem summary: "1 error, 2 warnings · …", each count in
//! its severity's color, then an unplaced problem's message or a hint
//! (on its own line on a phone, where the footer is narrow).

use leptos::prelude::*;

use super::super::diagnostics::{Diagnostic, MORE_PROBLEMS_CODE, Severity};
use super::super::state::EditorState;
use crate::EditorStrings;
use crate::strings::{counted, counted_as, fill};

/// The summary button; a click moves to the first placed problem.
#[component]
pub(super) fn ProblemCount(state: EditorState) -> impl IntoView {
    let diagnostics = state.problems().diagnostics;
    let first_placed = move |_| {
        let index = diagnostics.with_untracked(|problems| {
            problems
                .iter()
                .position(|problem| problem.line.is_some() && problem.column.is_some())
        });
        if let Some(index) = index {
            state.go_to_diagnostic(index);
        }
    };
    let summary = Memo::new(move |_| {
        state.strings(|strings| diagnostics.with(|problems| summary_parts(strings, problems)))
    });
    // The whole summary, for when the footer is too narrow to show it.
    let title =
        move || summary.with(|summary| (!summary.counts.is_empty()).then(|| summary.text()));
    let parts = move || summary_view(summary.get());
    view! {
        <button
            type="button"
            class="config-editor__diagnostic-status"
            disabled=move || diagnostics.with(Vec::is_empty)
            aria-live="polite"
            title=title
            on:click=first_placed
        >
            {parts}
        </button>
    }
}

/// The summary's pieces as elements: the counts, each classed by its
/// severity, then the detail with its separator (hidden when the detail
/// goes on a line of its own).
fn summary_view(summary: Summary) -> impl IntoView {
    // "No errors" stands alone.
    let after_counts = !summary.counts.is_empty();
    let counts = summary
        .counts
        .into_iter()
        .enumerate()
        .map(|(index, (severity, text))| {
            let class = format!(
                "config-editor__diagnostic-count config-editor__diagnostic-count-{}",
                severity.as_str()
            );
            view! {
                {(index > 0).then_some(", ")}
                <span class=class>{text}</span>
            }
        })
        .collect_view();
    let detail = summary.detail.map(|detail| {
        view! {
            <span class="config-editor__diagnostic-detail">
                {after_counts.then(|| view! { <span class="config-editor__diagnostic-separator">" · "</span> })}
                {detail}
            </span>
        }
    });
    (counts, detail)
}

/// The footer's problem summary in pieces.
#[derive(Clone, Debug, Default, PartialEq)]
struct Summary {
    /// "2 errors", "20+ warnings": each kind of problem there is, in order.
    counts: Vec<(Severity, String)>,
    /// What follows the counts (or "No errors" alone).
    detail: Option<String>,
}

impl Summary {
    /// As one text.
    fn text(&self) -> String {
        let counts: Vec<&str> = self.counts.iter().map(|(_, text)| text.as_str()).collect();
        match (&self.detail, counts.is_empty()) {
            (Some(detail), true) => detail.clone(),
            (Some(detail), false) => format!("{} · {detail}", counts.join(", ")),
            (None, _) => counts.join(", "),
        }
    }
}

/// The note `validate` adds when it cut the list.
fn is_more_note(problem: &Diagnostic) -> bool {
    problem.code.as_deref() == Some(MORE_PROBLEMS_CODE)
}

/// The counts, then an unplaced problem's message or a hint.
fn summary_parts(strings: &EditorStrings, problems: &[Diagnostic]) -> Summary {
    // The notes that more problems follow are not among them: the count of
    // the kind each note cut says "20+" instead.
    let cut = |severity: Severity| {
        problems
            .iter()
            .any(|problem| is_more_note(problem) && problem.severity == severity)
    };
    let more = problems.iter().any(is_more_note);
    let counted_problems: Vec<&Diagnostic> = problems
        .iter()
        .filter(|problem| !is_more_note(problem))
        .collect();
    let count_of = |severity: Severity| {
        counted_problems
            .iter()
            .filter(|problem| problem.severity == severity)
            .count()
    };
    // "20+ errors, 3 warnings": the "+" on each kind a cap cut.
    let counts: Vec<(Severity, String)> = [
        (Severity::Error, &strings.errors_one, &strings.errors_other),
        (
            Severity::Warning,
            &strings.warnings_one,
            &strings.warnings_other,
        ),
        (Severity::Info, &strings.infos_one, &strings.infos_other),
        (Severity::Hint, &strings.hints_one, &strings.hints_other),
    ]
    .into_iter()
    .map(|(severity, one, other)| (severity, count_of(severity), cut(severity), one, other))
    .filter(|(_, count, ..)| *count > 0)
    .map(|(severity, count, cut, one, other)| {
        let text = if cut {
            counted_as(one, other, count, &format!("{count}+"))
        } else {
            counted(one, other, count)
        };
        (severity, text)
    })
    .collect();
    if counts.is_empty() {
        return Summary {
            counts,
            detail: Some(strings.no_problems.clone()),
        };
    }
    // An unplaced problem (the whole document's) has no underline: it stays
    // in the footer even when the list was cut.
    let unplaced = counted_problems
        .iter()
        .find(|problem| problem.line.is_none());
    let cut_note = more.then(|| fill(&strings.problems_cut, &[("count", &counted_problems.len())]));
    let detail = match (unplaced, cut_note) {
        (Some(unplaced), Some(note)) => format!("{} · {note}", unplaced.message),
        (Some(unplaced), None) => unplaced.message.clone(),
        (None, Some(note)) => note,
        (None, None) => strings.problems_hint.clone(),
    };
    Summary {
        counts,
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::diagnostics::{Diagnostic, MORE_PROBLEMS_CODE, Severity};
    use super::{Summary, summary_parts};

    /// "2 errors, 1 warning · …" as one text.
    fn summary(strings: &EditorStrings, problems: &[Diagnostic]) -> String {
        summary_parts(strings, problems).text()
    }
    use crate::EditorStrings;

    /// QA 18: a whole document's problem stays in the footer when the list
    /// was cut, before the note, and the note counts it as F8 does.
    #[test]
    fn an_unplaced_problem_stays_shown_when_the_list_is_cut() {
        let mut problems: Vec<Diagnostic> = (0..20).map(|_| placed(Severity::Error)).collect();
        problems.push(more_note());
        problems.push(Diagnostic {
            message: "restart needed to apply".into(),
            severity: Severity::Warning,
            ..Diagnostic::default()
        });
        assert_eq!(
            summary(&EditorStrings::default(), &problems),
            "20+ errors, 1 warning · restart needed to apply · only the first 21 are shown"
        );
    }

    /// Each count keeps its severity, so the footer colors each by its own
    /// (a warning is never shown in the error color).
    #[test]
    fn each_count_keeps_its_severity() {
        let problems: Vec<Diagnostic> = [
            Severity::Warning,
            Severity::Error,
            Severity::Hint,
            Severity::Warning,
            Severity::Info,
        ]
        .into_iter()
        .map(placed)
        .collect();
        let found = summary_parts(&EditorStrings::default(), &problems);
        assert_eq!(
            found,
            Summary {
                counts: vec![
                    (Severity::Error, "1 error".to_string()),
                    (Severity::Warning, "2 warnings".to_string()),
                    (Severity::Info, "1 info".to_string()),
                    (Severity::Hint, "1 hint".to_string()),
                ],
                detail: Some(EditorStrings::default().problems_hint),
            }
        );
        assert_eq!(
            summary_parts(&EditorStrings::default(), &[]),
            Summary {
                counts: Vec::new(),
                detail: Some("No errors".to_string()),
            }
        );
    }

    fn placed(severity: Severity) -> Diagnostic {
        Diagnostic {
            line: Some(1),
            column: Some(1),
            end_column: Some(2),
            severity,
            ..Diagnostic::default()
        }
    }

    #[test]
    fn every_severity_is_counted() {
        let strings = EditorStrings::default();
        let cases: [(&[Severity], &str); 5] = [
            (&[], "No errors"),
            (&[Severity::Error, Severity::Error], "2 errors"),
            (&[Severity::Warning], "1 warning"),
            (&[Severity::Info, Severity::Info], "2 info"),
            (
                &[
                    Severity::Hint,
                    Severity::Error,
                    Severity::Info,
                    Severity::Warning,
                    Severity::Hint,
                ],
                "1 error, 1 warning, 1 info, 2 hints",
            ),
        ];
        let mut failures = Vec::new();
        for (severities, expected) in cases {
            let problems: Vec<Diagnostic> = severities.iter().copied().map(placed).collect();
            let found = summary(&strings, &problems);
            if !found.starts_with(expected) {
                failures.push(format!(
                    "{severities:?}: expected {expected:?}, found {found:?}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// The note a cap adds, cutting problems of `severity`.
    fn cut_note(severity: Severity) -> Diagnostic {
        Diagnostic {
            message: "Only the first 20 problems are shown; more follow.".to_string(),
            severity,
            code: Some(MORE_PROBLEMS_CODE.to_string()),
            ..Diagnostic::default()
        }
    }

    fn more_note() -> Diagnostic {
        cut_note(Severity::Error)
    }

    /// QA 17: the "+" goes on each kind a cap cut (schema errors, secret
    /// placeholder warnings), never at the end of the list, and "only the
    /// first N" counts the problems shown, in every combination.
    #[test]
    fn a_cut_list_says_which_kinds_have_more() {
        let strings = EditorStrings::default();
        // The problems shown by kind, the kinds a cap cut, and the footer.
        type Case = (
            &'static [(Severity, usize)],
            &'static [Severity],
            &'static str,
        );
        let cases: [Case; 6] = [
            (
                &[(Severity::Error, 20)],
                &[Severity::Error],
                "20+ errors · only the first 20 are shown",
            ),
            (
                &[(Severity::Warning, 20)],
                &[Severity::Warning],
                "20+ warnings · only the first 20 are shown",
            ),
            (
                &[(Severity::Error, 20), (Severity::Warning, 20)],
                &[Severity::Error, Severity::Warning],
                "20+ errors, 20+ warnings · only the first 40 are shown",
            ),
            (
                &[(Severity::Error, 20), (Severity::Warning, 20)],
                &[Severity::Warning],
                "20 errors, 20+ warnings · only the first 40 are shown",
            ),
            (
                &[(Severity::Error, 20), (Severity::Warning, 20)],
                &[Severity::Error],
                "20+ errors, 20 warnings · only the first 40 are shown",
            ),
            (
                &[(Severity::Hint, 1), (Severity::Error, 20)],
                &[Severity::Error],
                "20+ errors, 1 hint · only the first 21 are shown",
            ),
        ];
        let mut failures = Vec::new();
        for (shown, cut, expected) in cases {
            let mut problems: Vec<Diagnostic> = shown
                .iter()
                .flat_map(|(severity, count)| (0..*count).map(|_| placed(*severity)))
                .collect();
            problems.extend(cut.iter().copied().map(cut_note));
            let found = summary(&strings, &problems);
            if found != expected {
                failures.push(format!("expected {expected:?}, found {found:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Translated texts: the counts pick their singular or plural text, and
    /// the cut list is told apart by its code, whatever its message says.
    #[test]
    fn counts_follow_the_strings_setting() {
        let strings = EditorStrings {
            no_problems: "Keine Fehler".into(),
            errors_one: "{count} Fehler".into(),
            errors_other: "{count} Fehler (viele)".into(),
            warnings_one: "{count} Warnung".into(),
            problems_hint: "Zeiger auf Unterstrichenes".into(),
            problems_cut: "nur die ersten {count}".into(),
            ..EditorStrings::default()
        };
        assert_eq!(summary(&strings, &[]), "Keine Fehler");
        assert_eq!(
            summary(&strings, &[placed(Severity::Error)]),
            "1 Fehler · Zeiger auf Unterstrichenes"
        );
        assert_eq!(
            summary(
                &strings,
                &[placed(Severity::Error), placed(Severity::Warning)]
            ),
            "1 Fehler, 1 Warnung · Zeiger auf Unterstrichenes"
        );
        let mut problems: Vec<Diagnostic> = (0..20).map(|_| placed(Severity::Error)).collect();
        problems.push(Diagnostic {
            message: "Nur die ersten 20".into(),
            ..more_note()
        });
        assert_eq!(
            summary(&strings, &problems),
            "20+ Fehler (viele) · nur die ersten 20"
        );
    }
}
