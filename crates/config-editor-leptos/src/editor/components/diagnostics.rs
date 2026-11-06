//! Underlines for problems, each with its message as a tooltip.

use leptos::prelude::*;

use super::super::completion::visual_column;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::metrics::read_editor_metrics;
use super::super::state::EditorState;

/// Widest a diagnostic message gets, in pixels (`max-width` in preset.css).
const TOOLTIP_MAX_WIDTH: f64 = 360.0;

#[component]
pub(crate) fn DiagnosticOverlay(state: EditorState) -> impl IntoView {
    view! {
        <div
            class="config-editor__diagnostic-overlay"
            aria-label=move || format!("Inline {} diagnostics", state.label())
            style:transform=move || format!("translate({}px, {}px)", -state.scroll_x.get(), -state.scroll_y.get())
        >
            {move || {
                let text = state.source.get();
                state
                    .diagnostics
                    .get()
                    .into_iter()
                    .filter_map(|diagnostic| marker(state, &text, diagnostic))
                    .collect_view()
            }}
        </div>
    }
}

fn marker(state: EditorState, text: &str, diagnostic: Diagnostic) -> Option<impl IntoView + use<>> {
    let line = diagnostic.line?;
    let column = diagnostic.column?;
    let line_text = text.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let width = diagnostic
        .end_column
        .unwrap_or(column + 1)
        .saturating_sub(column)
        .max(1);
    let marked: String = line_text
        .chars()
        .skip(column.saturating_sub(1))
        .take(width)
        .collect();
    let at_end = marked.is_empty();
    let before: String = line_text.chars().take(column.saturating_sub(1)).collect();
    // Rendered columns, so tabs (two columns wide) do not shift the underline.
    let left = visual_column(&before);
    let visual_width = visual_column(&format!("{before}{marked}")) - left;
    let warning = diagnostic.severity == Severity::Warning;
    Some(view! {
        <button
            type="button"
            class="config-editor__diagnostic-marker"
            class:config-editor__diagnostic-marker-warning=warning
            class:config-editor__diagnostic-marker-eof=at_end
            class:config-editor__diagnostic-marker-shown=move || state.shown_problem.get() == Some((line, column))
            // Near the top of the viewport (not of the document), the message
            // opens below its line.
            class:config-editor__diagnostic-marker-near-top=move || {
                let line_height = state
                    .textarea()
                    .map_or(22.0, |editor| read_editor_metrics(&editor).line_height);
                (line as f64 - 1.0) * line_height - state.scroll_y.get() < 3.0 * line_height
            }
            tabindex="-1"
            aria-label=format!(
                "{} {} on line {line}: {}",
                state.label(),
                if warning { "warning" } else { "error" },
                diagnostic.message,
            )
            data-message=diagnostic.message
            style=format!(
                "left: {left}ch; top: calc(var(--config-editor-padding-top) + {} * var(--config-editor-line-height)); width: {}ch;",
                line.saturating_sub(1),
                visual_width.max(1.0),
            )
            // Shifted left, and no wider than the visible text area, so the
            // message is never clipped.
            style:--config-editor-diagnostic-tooltip-shift=move || tooltip_placement(state, left).0
            style:--config-editor-diagnostic-tooltip-width=move || tooltip_placement(state, left).1
            on:mousedown=|event| event.prevent_default()
            on:click=move |_| {
                state.focus_at(line, column);
            }
        >
            <span>{marked}</span>
        </button>
    })
}

/// `(shift, width)` CSS lengths for a message starting at visual column
/// `column`: as wide as the visible text area allows (up to
/// [`TOOLTIP_MAX_WIDTH`]) and shifted left to stay inside it.
fn tooltip_placement(state: EditorState, column: f64) -> (String, String) {
    let scroll = state.scroll_x.get();
    let Some(editor) = state.textarea() else {
        return ("0px".to_string(), format!("{TOOLTIP_MAX_WIDTH}px"));
    };
    let metrics = read_editor_metrics(&editor);
    let visible = f64::from(editor.client_width());
    let width = TOOLTIP_MAX_WIDTH.min(visible - 16.0).max(120.0);
    let left = metrics.padding_left + column * metrics.char_width - scroll;
    let shift = (visible - 8.0 - left - width).min(0.0).max(8.0 - left);
    (format!("{shift}px"), format!("{width}px"))
}
