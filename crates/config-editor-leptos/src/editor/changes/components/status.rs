//! The footer's change count.

use leptos::prelude::*;

use super::super::super::change_summary::ChangeSummary;
use super::super::super::state::EditorState;
use crate::EditorStrings;
use crate::strings::{counted, fill};

/// "2 changes" against the baseline; it moves to the next one, as `Alt+F5`.
#[component]
pub(in crate::editor) fn ChangeStatus(state: EditorState) -> impl IntoView {
    let changes = state.changes().summary;
    let next_change = move |_| state.go_to_change(true, false);
    move || {
        let summary = changes.get();
        (!summary.is_empty()).then(|| {
            // Taken by the button, which goes with the next count (see
            // `reactive`).
            let strings = state.strings_signal();
            view! {
                <button
                    type="button"
                    class="config-editor__change-status"
                    title=move || strings.with(|strings| change_details(strings, &summary))
                    on:click=next_change
                >
                    {move || strings.with(|strings| change_count(strings, &summary))}
                </button>
            }
        })
    }
}

/// "2 changes".
fn change_count(strings: &EditorStrings, summary: &ChangeSummary) -> String {
    counted(&strings.changes_one, &strings.changes_other, summary.hunks)
}

/// "1 added, 2 modified, 1 removed lines · Alt+F5 for the next".
fn change_details(strings: &EditorStrings, summary: &ChangeSummary) -> String {
    let parts: Vec<String> = [
        (summary.added, &strings.changed_added),
        (summary.modified, &strings.changed_modified),
        (summary.removed, &strings.changed_removed),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, template)| fill(template, &[("count", &count)]))
    .collect();
    let lines = summary.added + summary.modified + summary.removed;
    let template = if lines == 1 {
        &strings.change_details_one
    } else {
        &strings.change_details_other
    };
    fill(template, &[("lines", &parts.join(", "))])
}
