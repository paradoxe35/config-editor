//! The footer's change count.

use leptos::prelude::*;

use super::super::super::change_summary::ChangeSummary;
use super::super::super::state::EditorState;

/// "2 changes" against the baseline; it moves to the next one, as `Alt+F5`.
#[component]
pub(in crate::editor) fn ChangeStatus(state: EditorState) -> impl IntoView {
    let changes = state.changes.summary;
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
