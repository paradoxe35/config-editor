//! The built-in footer: the suggestions hint, problem counts and line count.

use leptos::prelude::*;

use crate::reactive::Kept;

use super::super::a11y::toggle_tab_focus;
use super::super::changes::ChangeStatus;
use super::super::diagnostics::{Diagnostic, MORE_PROBLEMS_CODE, Severity};
use super::super::state::{EditorState, line_count};
use crate::EditorStrings;
use crate::strings::{counted, counted_as, fill};

#[component]
pub(crate) fn StatusBar(state: EditorState) -> impl IntoView {
    let diagnostics = state.problems.diagnostics;
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
    let offers_completions =
        move || state.config.settings.completions.get() && !state.config.read_only.get();
    let problem_count =
        move || state.strings(|strings| diagnostics.with(|problems| summary(strings, problems)));
    let line_total = move || {
        let count = state.doc.text.with(|text| line_count(text));
        state.strings(|strings| counted(&strings.lines_one, &strings.lines_other, count))
    };
    view! {
        <div class="config-editor__status">
            <span class="config-editor__hint">
                {move || offers_completions().then(|| view! { <SuggestionsHint state=state /> })}
            </span>
            <span class="config-editor__status-meta">
                <TabFocusStatus state=state />
                <Kept when=state.config.settings.diagnostics view=move || view! {
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
                        {problem_count}
                    </button>
                } />
                <Kept when=state.config.settings.diff view=move || view! { <ChangeStatus state=state /> } />
                <span class="config-editor__line-count">{line_total}</span>
            </span>
        </div>
    }
}

/// "Ctrl + Space for suggestions": the `suggestionsHint` text with the
/// `suggestionsKey` chord drawn as keys where it says `{key}`.
#[component]
fn SuggestionsHint(state: EditorState) -> impl IntoView {
    let strings = state.strings_signal();
    let pieces = move || strings.with(|strings| hint_pieces(strings));
    move || {
        pieces()
            .into_iter()
            .map(|piece| match piece {
                HintPiece::Key(key) => view! { <kbd>{key}</kbd> }.into_any(),
                HintPiece::Text(text) => view! { <span>{text}</span> }.into_any(),
            })
            .collect_view()
    }
}

/// A piece of the suggestions hint.
#[derive(Debug, PartialEq)]
enum HintPiece {
    Key(String),
    Text(String),
}

/// The hint as text and keys: "Ctrl", "+", "Space", "for suggestions".
fn hint_pieces(strings: &EditorStrings) -> Vec<HintPiece> {
    let hint = &strings.suggestions_hint;
    let (before, after) = hint.split_once("{key}").unwrap_or(("", hint.as_str()));
    let mut pieces = Vec::new();
    let text = |text: &str| {
        let text = text.trim();
        (!text.is_empty()).then(|| HintPiece::Text(text.to_string()))
    };
    pieces.extend(text(before));
    for (index, key) in strings.suggestions_key.split('+').enumerate() {
        if index > 0 {
            pieces.push(HintPiece::Text("+".to_string()));
        }
        pieces.push(HintPiece::Key(key.trim().to_string()));
    }
    pieces.extend(text(after));
    pieces
}

/// "Tab moves focus" while `Ctrl+M` is on; clicking it turns it off.
#[component]
fn TabFocusStatus(state: EditorState) -> impl IntoView {
    let turn_off = move |_| toggle_tab_focus(state);
    move || {
        state.a11y.tab_moves_focus.get().then(|| {
            view! {
                <button
                    type="button"
                    class="config-editor__tab-focus-status"
                    title=state.text(|strings| &strings.tab_focus_title)
                    on:click=turn_off
                >
                    {state.text(|strings| &strings.tab_focus)}
                </button>
            }
        })
    }
}

/// The note `validate` adds when it cut the list.
fn is_more_note(problem: &Diagnostic) -> bool {
    problem.code.as_deref() == Some(MORE_PROBLEMS_CODE)
}

/// "2 errors, 1 warning · …": the counts, then an unplaced problem's
/// message or a hint.
fn summary(strings: &EditorStrings, problems: &[Diagnostic]) -> String {
    // The note that more problems follow is not one of them: the counts say
    // "20+" instead.
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
    let counts: Vec<(usize, &String, &String)> = [
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
    .map(|(severity, one, other)| (count_of(severity), one, other))
    .filter(|(count, _, _)| *count > 0)
    .collect();
    let count = match counts.as_slice() {
        [] => return strings.no_problems.clone(),
        // "20+ errors"; with several kinds the "+" ends the list.
        [(count, one, other)] if more => counted_as(one, other, *count, &format!("{count}+")),
        _ => {
            let listed: Vec<String> = counts
                .iter()
                .map(|(count, one, other)| counted(one, other, *count))
                .collect();
            format!("{}{}", listed.join(", "), if more { "+" } else { "" })
        }
    };
    let detail = if more {
        fill(&strings.problems_cut, &[("count", &counted_problems.len())])
    } else {
        match problems.iter().find(|problem| problem.line.is_none()) {
            Some(unplaced) => unplaced.message.clone(),
            None => strings.problems_hint.clone(),
        }
    };
    format!("{count} · {detail}")
}

#[cfg(test)]
mod tests {
    use super::super::super::diagnostics::{Diagnostic, MORE_PROBLEMS_CODE, Severity};
    use super::{HintPiece, hint_pieces, summary};
    use crate::EditorStrings;

    #[test]
    fn the_hint_draws_its_chord_as_keys_where_it_says() {
        let text = |text: &str| HintPiece::Text(text.to_string());
        let key = |key: &str| HintPiece::Key(key.to_string());
        assert_eq!(
            hint_pieces(&EditorStrings::default()),
            [
                key("Ctrl"),
                text("+"),
                key("Space"),
                text("for suggestions")
            ]
        );
        let german = EditorStrings {
            suggestions_hint: "Vorschläge mit {key}".into(),
            suggestions_key: "Strg+Leertaste".into(),
            ..EditorStrings::default()
        };
        assert_eq!(
            hint_pieces(&german),
            [
                text("Vorschläge mit"),
                key("Strg"),
                text("+"),
                key("Leertaste")
            ]
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

    fn more_note() -> Diagnostic {
        Diagnostic {
            message: "Only the first 20 problems are shown; more follow.".to_string(),
            severity: Severity::Warning,
            code: Some(MORE_PROBLEMS_CODE.to_string()),
            ..Diagnostic::default()
        }
    }

    #[test]
    fn a_cut_list_says_more_follow() {
        let strings = EditorStrings::default();
        let mut problems: Vec<Diagnostic> = (0..20).map(|_| placed(Severity::Error)).collect();
        problems.push(more_note());
        assert_eq!(
            summary(&strings, &problems),
            "20+ errors · only the first 20 are shown"
        );
        problems.insert(0, placed(Severity::Hint));
        assert!(summary(&strings, &problems).starts_with("20 errors, 1 hint+"));
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
