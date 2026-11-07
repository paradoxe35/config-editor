//! The built-in footer: the suggestions hint, problem counts and line count.

use leptos::prelude::*;

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
    view! {
        <div class="config-editor__status">
            <span class="config-editor__hint">
                {state.features.completions.then(|| view! {
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

/// The note `validate` adds when it cut the list.
fn is_more_note(problem: &Diagnostic) -> bool {
    problem.line.is_none() && problem.message.ends_with("more follow.")
}

/// "2 errors, 1 warning · …": the counts, then an unplaced problem's
/// message or a hint.
fn summary(problems: &[Diagnostic]) -> String {
    let plural = |count: usize| if count == 1 { "" } else { "s" };
    // The note that more problems follow is not one of them: the counts say
    // "20+" instead.
    let more = problems.iter().any(is_more_note);
    let counted: Vec<&Diagnostic> = problems.iter().filter(|problem| !is_more_note(problem)).collect();
    let warnings = counted.iter().filter(|problem| problem.severity == Severity::Warning).count();
    let errors = counted.len() - warnings;
    let plus = if more { "+" } else { "" };
    let count = match (errors, warnings) {
        (0, 0) => return "No errors".to_string(),
        (errors, 0) => format!("{errors}{plus} error{}", plural(errors)),
        (0, warnings) => format!("{warnings}{plus} warning{}", plural(warnings)),
        (errors, warnings) => format!(
            "{errors} error{}, {warnings} warning{}{plus}",
            plural(errors),
            plural(warnings)
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
