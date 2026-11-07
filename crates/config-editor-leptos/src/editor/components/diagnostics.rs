//! Underlines for problems, each with its message as a tooltip.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::super::completion::visual_column;
use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
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
    let severity = diagnostic.severity;
    let from_host = diagnostic.origin == DiagnosticOrigin::Host;
    let detail = detail(&diagnostic);
    let docs_url = diagnostic.docs_url.clone();
    let has_docs = docs_url.is_some();
    let aria_label = format!(
        "{} {}{} on line {line}: {}",
        state.label(),
        severity.as_str(),
        diagnostic
            .source
            .as_deref()
            .filter(|_| from_host)
            .map(|source| format!(" from {source}"))
            .unwrap_or_default(),
        diagnostic.message,
    );
    Some(view! {
        <button
            type="button"
            class="config-editor__diagnostic-marker"
            class:config-editor__diagnostic-marker-warning=severity == Severity::Warning
            class:config-editor__diagnostic-marker-info=severity == Severity::Info
            class:config-editor__diagnostic-marker-hint=severity == Severity::Hint
            class:config-editor__diagnostic-marker-host=from_host
            class:config-editor__diagnostic-marker-docs=has_docs
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
            aria-label=aria_label
            data-message=diagnostic.message
            data-detail=detail
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
            on:click=move |event| on_marker_click(state, &event, line, column, docs_url.as_deref())
        >
            <span>{marked}</span>
        </button>
    })
}

/// The line under a host problem's message: who reported it, its code, and
/// a docs link (the message itself opens it).
fn detail(diagnostic: &Diagnostic) -> Option<String> {
    if diagnostic.origin != DiagnosticOrigin::Host {
        return None;
    }
    let parts: Vec<&str> = [
        diagnostic.source.as_deref(),
        diagnostic.code.as_deref(),
        diagnostic.docs_url.as_ref().map(|_| "Open docs ↗"),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(parts.join(" · "))
}

/// A click on the underline moves the caret there; one on the message of a
/// problem with docs (drawn outside the underline's box) opens them.
fn on_marker_click(
    state: EditorState,
    event: &web_sys::MouseEvent,
    line: usize,
    column: usize,
    docs_url: Option<&str>,
) {
    let on_message = event
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|marker| {
            let rect = marker.get_bounding_client_rect();
            let y = f64::from(event.client_y());
            y < rect.top() || y > rect.bottom()
        });
    if let (true, Some(url)) = (on_message, docs_url) {
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target_and_features(url, "_blank", "noopener");
        }
        return;
    }
    state.focus_at(line, column);
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
