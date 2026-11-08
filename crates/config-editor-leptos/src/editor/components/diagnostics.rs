//! Underlines for problems, each with its message as a tooltip.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::super::a11y::problem_marker;
use super::super::completion::visual_column;
use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use super::super::metrics::read_editor_metrics;
use super::super::state::EditorState;
use crate::EditorStrings;
use crate::reactive::Relay;

/// Widest a diagnostic message gets, in pixels (`max-width` in preset.css).
const TOOLTIP_MAX_WIDTH: f64 = 360.0;

/// What the underlines follow besides their own problem. They are redrawn
/// after every check, so they read it through relays (see `reactive`).
#[derive(Clone, Copy)]
struct Follows {
    /// The problem whose message is shown.
    shown: Relay<Option<(usize, usize)>>,
    scroll_x: Relay<f64>,
    scroll_y: Relay<f64>,
    frame: Relay<Option<Frame>>,
}

/// [`Follows`], as signals of one drawing of the underlines.
#[derive(Clone, Copy)]
struct Followed {
    shown: Signal<Option<(usize, usize)>>,
    scroll_x: Signal<f64>,
    scroll_y: Signal<f64>,
    frame: Signal<Option<Frame>>,
}

/// The text area's measures the underlines' messages are placed by: read
/// once when the layout changes, not by each underline (hundreds of reads
/// of the layout, each after the last underline's style was written, made
/// drawing many problems slow).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Frame {
    char_width: f64,
    line_height: f64,
    padding_left: f64,
    /// The text area's width inside its borders.
    visible: f64,
}

impl Frame {
    /// Measured now; tracks what moves the text area (a window resize or
    /// scroll, the theme, the gutter's settings).
    fn measure(state: EditorState) -> Option<Self> {
        state.view().layout_revision.track();
        let settings = &state.config().settings;
        settings.theme.track();
        settings.styled.track();
        settings.folding.track();
        settings.diff.track();
        let editor = state.view().textarea_ref.get()?;
        let metrics = read_editor_metrics(&editor);
        Some(Frame {
            char_width: metrics.char_width,
            line_height: metrics.line_height,
            padding_left: metrics.padding_left,
            visible: f64::from(editor.client_width()),
        })
    }
}

impl Follows {
    fn read(&self) -> Followed {
        Followed {
            shown: self.shown.read(),
            scroll_x: self.scroll_x.read(),
            scroll_y: self.scroll_y.read(),
            frame: self.frame.read(),
        }
    }
}

#[component]
pub(crate) fn DiagnosticOverlay(state: EditorState) -> impl IntoView {
    let follows = Follows {
        shown: Relay::new(move || state.problems().current.get()),
        scroll_x: Relay::new(move || state.view().scroll_x.get()),
        scroll_y: Relay::new(move || state.view().scroll_y.get()),
        frame: Relay::new(move || Frame::measure(state)),
    };
    view! {
        <div
            class="config-editor__diagnostic-overlay"
            aria-label=state.language_text(|strings| &strings.problems_label)
            style:transform=move || format!("translate({}px, {}px)", -state.view().scroll_x.get(), -state.view().scroll_y.get())
        >
            {move || {
                let text = state.doc().text.get();
                let projection = state.view().projection.get();
                let followed = follows.read();
                // New texts redraw the underlines' names and details.
                state.config().settings.strings.track();
                // Each underline finds its line here, not by counting lines.
                let lines: Vec<&str> = text.split('\n').collect();
                state
                    .problems().diagnostics
                    .get()
                    .into_iter()
                    .filter_map(|diagnostic| {
                        // A folded line's problem shows on the fold's line
                        // in the gutter, not as an underline.
                        let line = diagnostic.line?.saturating_sub(1);
                        if projection.is_hidden(line) {
                            return None;
                        }
                        let line_text = lines.get(line).copied().unwrap_or("");
                        marker(state, followed, line_text, diagnostic, projection.row_of_line(line))
                    })
                    .collect_view()
            }}
        </div>
    }
}

/// The underline for `diagnostic`, drawn on textarea row `row`.
fn marker(
    state: EditorState,
    followed: Followed,
    line_text: &str,
    diagnostic: Diagnostic,
    row: usize,
) -> Option<impl IntoView + use<>> {
    let line = diagnostic.line?;
    let column = diagnostic.column?;
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
    let detail = state.strings_untracked(|strings| detail(strings, &diagnostic));
    let docs_url = diagnostic.docs_url.clone();
    let has_docs = docs_url.is_some();
    let aria_label = state
        .strings_untracked(|strings| problem_marker(strings, state.label(), &diagnostic, line));
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
            class:config-editor__diagnostic-marker-shown=move || followed.shown.get() == Some((line, column))
            // Near the top of the viewport (not of the document), the message
            // opens below its line.
            class:config-editor__diagnostic-marker-near-top=move || {
                let line_height = followed.frame.get().map_or(22.0, |frame| frame.line_height);
                row as f64 * line_height - followed.scroll_y.get() < 3.0 * line_height
            }
            tabindex="-1"
            aria-label=aria_label
            data-message=diagnostic.message
            data-detail=detail
            style=format!(
                "left: {left}ch; top: calc(var(--config-editor-padding-top) + {} * var(--config-editor-line-height)); width: {}ch;",
                row,
                visual_width.max(1.0),
            )
            // Shifted left, and no wider than the visible text area, so the
            // message is never clipped.
            style:--config-editor-diagnostic-tooltip-shift=move || tooltip_placement(followed.frame.get(), followed.scroll_x.get(), left).0
            style:--config-editor-diagnostic-tooltip-width=move || tooltip_placement(followed.frame.get(), followed.scroll_x.get(), left).1
            on:mousedown=|event| event.prevent_default()
            on:click=move |event| on_marker_click(state, &event, line, column, docs_url.as_deref())
        >
            <span>{marked}</span>
        </button>
    })
}

/// The line under a host problem's message: who reported it, its code, and
/// a docs link (the message itself opens it).
fn detail(strings: &EditorStrings, diagnostic: &Diagnostic) -> Option<String> {
    if diagnostic.origin != DiagnosticOrigin::Host {
        return None;
    }
    let parts: Vec<&str> = [
        diagnostic.source.as_deref(),
        diagnostic.code.as_deref(),
        diagnostic
            .docs_url
            .as_ref()
            .map(|_| strings.open_docs.as_str()),
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
    // Its message stays shown, as after F8: a tap on a touch screen has
    // no hover to show it with.
    state.problems().current.set(Some((line, column)));
}

/// `(shift, width)` CSS lengths for a message starting at visual column
/// `column`, the text scrolled `scroll` pixels sideways: as wide as the visible text area allows (up to
/// [`TOOLTIP_MAX_WIDTH`]) and shifted left to stay inside it.
fn tooltip_placement(frame: Option<Frame>, scroll: f64, column: f64) -> (String, String) {
    let Some(frame) = frame else {
        return ("0px".to_string(), format!("{TOOLTIP_MAX_WIDTH}px"));
    };
    let visible = frame.visible;
    let width = TOOLTIP_MAX_WIDTH.min(visible - 16.0).max(120.0);
    let left = frame.padding_left + column * frame.char_width - scroll;
    let shift = (visible - 8.0 - left - width).min(0.0).max(8.0 - left);
    (format!("{shift}px"), format!("{width}px"))
}
