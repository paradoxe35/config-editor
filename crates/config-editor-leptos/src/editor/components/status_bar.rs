//! The built-in footer: the suggestions hint, problem counts and line count.

use leptos::prelude::*;

use super::super::changes::ChangeSummary;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::state::EditorState;

#[component]
pub(crate) fn StatusBar(state: EditorState) -> impl IntoView {
    let diagnostics = state.diagnostics;
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
    // Follows `read_only`, which switches without remounting.
    let offers_completions = move || state.features.completions && !state.read_only.get();
    view! {
        <div class="config-editor__status">
            <span class="config-editor__hint">
                {move || offers_completions().then(|| view! {
                    <span><kbd>Ctrl</kbd><span>"+"</span><kbd>Space</kbd><span>"for suggestions"</span></span>
                })}
            </span>
            <span class="config-editor__status-meta">
                {state.features.diagnostics.then(|| view! {
                    <button
                        type="button"
                        class="config-editor__diagnostic-status"
                        class:config-editor__diagnostic-status-error=move || {
                            diagnostics.with(|problems| problems.iter().any(|problem| problem.severity == Severity::Error))
                        }
                        disabled=move || diagnostics.with(Vec::is_empty)
                        aria-live="polite"
                        title=move || {
                            diagnostics.with(|problems| {
                                problems
                                    .first()
                                    .filter(|problem| problem.line.is_none())
                                    .map(|problem| problem.message.clone())
                            })
                        }
                        on:click=first_placed
                    >
                        {move || diagnostics.with(|problems| summary(problems))}
                    </button>
                })}
                {state.features.diff.then(|| view! { <ChangeStatus state=state /> })}
                <span class="config-editor__line-count">
                    {move || {
                        let count = state.source.with(|text| text.split('\n').count());
                        format!("{count} {}", if count == 1 { "line" } else { "lines" })
                    }}
                </span>
            </span>
        </div>
    }
}

/// "2 changes" against the baseline; it moves to the next one, as `Alt+F5`.
#[component]
fn ChangeStatus(state: EditorState) -> impl IntoView {
    let changes = state.changes;
    let next_change = move |_| state.go_to_change(true, false);
    move || {
        let summary = changes.get();
        (!summary.is_empty()).then(|| {
            view! {
                <button
                    type="button"
                    class="config-editor__change-status"
                    title=change_details(&summary)
                    on:click=next_change
                >
                    {change_count(&summary)}
                </button>
            }
        })
    }
}

/// "2 changes".
fn change_count(summary: &ChangeSummary) -> String {
    let plural = if summary.hunks == 1 { "" } else { "s" };
    format!("{} change{plural}", summary.hunks)
}

/// "1 added, 2 modified, 1 removed lines · Alt+F5 for the next".
fn change_details(summary: &ChangeSummary) -> String {
    let parts: Vec<String> = [
        (summary.added, "added"),
        (summary.modified, "modified"),
        (summary.removed, "removed"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, what)| format!("{count} {what}"))
    .collect();
    let lines = if summary.added + summary.modified + summary.removed == 1 {
        "line"
    } else {
        "lines"
    };
    format!("{} {lines} · Alt+F5 for the next", parts.join(", "))
}

/// The note `validate` adds when it cut the list.
fn is_more_note(problem: &Diagnostic) -> bool {
    problem.line.is_none() && problem.message.ends_with("more follow.")
}

/// "2 errors, 1 warning · …": the counts, then an unplaced problem's
/// message or a hint.
fn summary(problems: &[Diagnostic]) -> String {
    // The note that more problems follow is not one of them: the counts say
    // "20+" instead.
    let more = problems.iter().any(is_more_note);
    let counted: Vec<&Diagnostic> = problems
        .iter()
        .filter(|problem| !is_more_note(problem))
        .collect();
    let count_of = |severity: Severity| {
        counted
            .iter()
            .filter(|problem| problem.severity == severity)
            .count()
    };
    let plus = if more { "+" } else { "" };
    let counts: Vec<(usize, &str, &str)> = [
        (Severity::Error, "error", "errors"),
        (Severity::Warning, "warning", "warnings"),
        (Severity::Info, "info", "info"),
        (Severity::Hint, "hint", "hints"),
    ]
    .into_iter()
    .map(|(severity, one, many)| (count_of(severity), one, many))
    .filter(|(count, _, _)| *count > 0)
    .collect();
    let named = |(count, one, many): (usize, &str, &str)| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    let count = match counts.as_slice() {
        [] => return "No errors".to_string(),
        // "20+ errors"; with several kinds the "+" ends the list.
        [(count, _, many)] if more => format!("{count}+ {many}"),
        _ => format!(
            "{}{plus}",
            counts.into_iter().map(named).collect::<Vec<_>>().join(", ")
        ),
    };
    if more {
        return format!("{count} · only the first {} are shown", counted.len());
    }
    match problems.iter().find(|problem| problem.line.is_none()) {
        Some(unplaced) => format!("{count} · {}", unplaced.message),
        None => format!("{count} · hover or focus underlined text"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::diagnostics::{Diagnostic, Severity};
    use super::summary;

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
            let found = summary(&problems);
            if !found.starts_with(expected) {
                failures.push(format!(
                    "{severities:?}: expected {expected:?}, found {found:?}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn a_cut_list_says_more_follow() {
        let mut problems: Vec<Diagnostic> = (0..20).map(|_| placed(Severity::Error)).collect();
        problems.push(Diagnostic {
            message: "Only the first 20 problems are shown; more follow.".to_string(),
            severity: Severity::Warning,
            ..Diagnostic::default()
        });
        assert_eq!(
            summary(&problems),
            "20+ errors · only the first 20 are shown"
        );
        problems.insert(0, placed(Severity::Hint));
        assert!(summary(&problems).starts_with("20 errors, 1 hint+"));
    }
}
