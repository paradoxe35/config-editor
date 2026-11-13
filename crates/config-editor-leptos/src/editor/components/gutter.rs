//! Line numbers with their error, lock and change markers.

use leptos::prelude::*;

use super::super::changes::ChangeMarkers;
use super::super::commands::line_range;
use super::super::diff::DiffKind;
use super::super::input;
use super::super::line_rows::{LineRows, RowUiState};
use super::super::state::{EditorState, line_count};
use super::row_state_for;
use crate::reactive::{KeyedList, MANY_ROWS, RowSpacer};

#[component]
pub(crate) fn Gutter(state: EditorState, rows: LineRows) -> impl IntoView {
    view! {
        <div
            class="config-editor__gutter"
            class:config-editor__gutter-folding=state.config().settings.folding
            on:mousemove=move |event| input::gutter_move(state, &event)
            on:mousedown=move |event| input::gutter_press(state, &event)
            // As wide as the longest line number (at least three digits);
            // the stylesheet adds the marker column and the padding.
            style:--config-editor-gutter-digits=move || {
                state.doc().text.with(|text| line_count(text)).to_string().len().max(3).to_string()
            }
        >
            <div
                class="config-editor__gutter-content"
                style:transform=move || format!("translateY(-{}px)", state.view().scroll_y.get())
            >
                <RowSpacer
                    rows=move || state.view().visible_rows.with(|rows| rows.start)
                    row_height=|| "1lh"
                />
                <KeyedList
                    each=move || state.view().visible_lines.get()
                    key=|index| *index
                    reserve=MANY_ROWS
                    children=move |index| view! { <LineNumber state=state rows=rows index=index /> }
                />
            </div>
        </div>
    }
}

#[component]
fn LineNumber(state: EditorState, rows: LineRows, index: usize) -> impl IntoView {
    let line = index + 1;
    let row = row_state_for(state, rows, index);
    let select_line = move |_| {
        // A line number selects its whole line, as in VS Code.
        let Some(offset) = state.focus_at(line, 1) else {
            return;
        };
        let (_, line_end) = state
            .doc()
            .text
            .with_untracked(|text| line_range(text, offset));
        if let Some(element) = state.textarea() {
            let _ = element.set_selection_range(offset as u32, line_end as u32);
        }
    };
    view! {
        <div
            data-line=line
            // One binding for every state class: rows are many, and each
            // binding is a reactive node of its own.
            class=move || row.with(line_number_class)
            on:click=select_line
        >
            <span aria-hidden="true">{line}</span>
            {move || {
                // A lock, else a key beside a secret placeholder: one node.
                row.with(|row| {
                    row.marks.as_ref().and_then(|marks| match (&marks.lock, &marks.key) {
                        (Some(lock), _) => Some((LOCK, lock.clone())),
                        (None, Some(key)) => Some((KEY, key.clone())),
                        (None, None) => None,
                    })
                })
                .map(|(icon, label)| view! {
                    <span class=icon.class role="img" aria-label=label.clone() title=label>
                        <svg viewBox=icon.view_box aria-hidden="true"><path d=icon.path /></svg>
                    </span>
                })
            }}
            <ChangeMarkers state=state row=row line=line />
        </div>
    }
}

/// A gutter icon, drawn as inline SVG (a strict CSP refuses `data:` images).
struct Icon {
    class: &'static str,
    view_box: &'static str,
    path: &'static str,
}

/// A read-only line's lock.
const LOCK: Icon = Icon {
    class: "config-editor__lock-marker",
    view_box: "0 0 16 18",
    path: "M3 8V5a5 5 0 0 1 10 0v3h1.5v10h-13V8zm2.5 0h5V5a2.5 2.5 0 0 0-5 0z",
};

/// A secret placeholder's key.
const KEY: Icon = Icon {
    class: "config-editor__secret-marker",
    view_box: "0 0 16 16",
    path: "M5 2a4 4 0 0 1 3.9 4.9L16 14v2h-3v-2h-2v-2H9.4L7 9.6A4 4 0 1 1 5 2zm-.5 2.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3z",
};

/// The state classes of a line number.
fn line_number_class(row: &RowUiState) -> &'static str {
    match (row.is_active, row.has_error || row.hidden_error, row.change) {
        (false, false, DiffKind::Unchanged) => "",
        (true, false, DiffKind::Unchanged) => "config-editor__line-number-active",
        (false, false, DiffKind::Added) => "config-editor__line-number-added",
        (true, false, DiffKind::Added) => {
            "config-editor__line-number-active config-editor__line-number-added"
        }
        (false, false, DiffKind::Modified) => "config-editor__line-number-modified",
        (true, false, DiffKind::Modified) => {
            "config-editor__line-number-active config-editor__line-number-modified"
        }
        (false, true, DiffKind::Unchanged) => "config-editor__line-number-error",
        (true, true, DiffKind::Unchanged) => {
            "config-editor__line-number-active config-editor__line-number-error"
        }
        (false, true, DiffKind::Added) => {
            "config-editor__line-number-error config-editor__line-number-added"
        }
        (true, true, DiffKind::Added) => {
            "config-editor__line-number-active config-editor__line-number-error config-editor__line-number-added"
        }
        (false, true, DiffKind::Modified) => {
            "config-editor__line-number-error config-editor__line-number-modified"
        }
        (true, true, DiffKind::Modified) => {
            "config-editor__line-number-active config-editor__line-number-error config-editor__line-number-modified"
        }
    }
}
