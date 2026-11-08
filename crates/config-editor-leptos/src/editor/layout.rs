//! What the view is laid out from: the elements, the scroll position, the
//! lines as rendered, and which textarea rows exist in the DOM.

use std::ops::Range;
use std::sync::Arc;

use leptos::prelude::*;

use super::highlight::comment_lines::CommentLines;
use super::languages::Language;
use super::line_rows::RenderedLine;
use super::metrics::read_editor_metrics;
use super::projection::Projection;

/// Rows rendered above and below the visible ones, so fast scrolling does
/// not show blank lines before the next frame.
const OVERSCAN_ROWS: usize = 40;

/// Rows rendered before the editor element exists to measure.
const INITIAL_ROWS: usize = 120;

/// The view part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct ViewState {
    /// The textarea; read it through `EditorState::textarea`, which maps
    /// its offsets past folded lines.
    pub(super) textarea_ref: NodeRef<leptos::html::Textarea>,
    pub(super) completion_ref: NodeRef<leptos::html::Div>,
    pub(super) root_ref: NodeRef<leptos::html::Div>,
    pub(super) scroll_x: RwSignal<f64>,
    pub(super) scroll_y: RwSignal<f64>,
    /// Bumped on window resize and scroll so popups re-measure.
    pub(super) layout_revision: RwSignal<usize>,
    /// Rows must not subscribe to this memo or the diff; the row sync effect
    /// fans them out to per-row signals (see `line_rows`).
    pub(super) lines: Memo<Vec<RenderedLine>>,
    /// Only the rows in view (plus overscan) exist in the DOM: textarea
    /// rows, which skip folded lines.
    pub(super) visible_rows: Memo<Range<usize>>,
    /// Lines inside multi-line strings' bodies, shown as string text.
    pub(super) string_bodies: Memo<Vec<Range<usize>>>,
    /// The lines past the first of comments written over several lines.
    pub(super) comment_lines: Memo<Vec<CommentLines>>,
    /// The textarea's text: the document with the closed folds cut out
    /// (see `projection`; the document itself without folding).
    pub(super) projection: RwSignal<Arc<Projection>>,
    /// The document lines (0-based) on the textarea rows in view.
    pub(super) visible_lines: Memo<Vec<usize>>,
}

impl ViewState {
    /// The view of the document, whose textarea shows `projection`, its lines
    /// rendered as `lines`, with the multi-line string bodies of
    /// [`string_bodies`] and the comments of [`comment_lines`].
    pub(super) fn new(
        projection: RwSignal<Arc<Projection>>,
        lines: Memo<Vec<RenderedLine>>,
        string_bodies: Memo<Vec<Range<usize>>>,
        comment_lines: Memo<Vec<CommentLines>>,
    ) -> Self {
        let textarea_ref: NodeRef<leptos::html::Textarea> = NodeRef::new();
        let scroll_y = RwSignal::new(0.0);
        let layout_revision = RwSignal::new(0usize);
        let visible_rows = Memo::new(move |_| {
            layout_revision.track();
            let top = scroll_y.get();
            // From the text, not `lines`: a second reader of `lines` here
            // kept the row sync from seeing diagnostic-only changes to it.
            let count = projection.with(|projection| projection.rows());
            let Some(editor) = textarea_ref.get() else {
                return 0..count.min(INITIAL_ROWS);
            };
            let metrics = read_editor_metrics(&editor);
            let window_height = web_sys::window()
                .and_then(|window| window.inner_height().ok())
                .and_then(|height| height.as_f64())
                .unwrap_or(0.0);
            let height = f64::from(editor.client_height()).max(window_height);
            let line_height = metrics.line_height.max(1.0);
            let first = ((top - metrics.padding_top) / line_height).floor().max(0.0) as usize;
            let first = first.saturating_sub(OVERSCAN_ROWS).min(count);
            let last =
                (first + (height / line_height).ceil() as usize + 2 * OVERSCAN_ROWS).min(count);
            first..last
        });
        let visible_lines = Memo::new(move |_| {
            let rows = visible_rows.get();
            projection.with(|projection| rows.map(|row| projection.line_of_row(row)).collect())
        });
        Self {
            textarea_ref,
            completion_ref: NodeRef::new(),
            root_ref: NodeRef::new(),
            scroll_x: RwSignal::new(0.0),
            scroll_y,
            layout_revision,
            lines,
            visible_rows,
            string_bodies,
            comment_lines,
            projection,
            visible_lines,
        }
    }
}

/// The lines inside multi-line strings' bodies of `text` (in
/// `edit_language`), found once per edit for the view and the suggestions.
pub(super) fn string_bodies(
    text: RwSignal<String>,
    edit_language: RwSignal<Language>,
) -> Memo<Vec<Range<usize>>> {
    Memo::new(move |_| text.with(|text| edit_language.get().behavior().string_body_lines(text)))
}

/// The lines past the first of the comments and strings `text` (in
/// `edit_language`) writes over several lines, found once per edit for the
/// code layer.
pub(super) fn comment_lines(
    text: RwSignal<String>,
    edit_language: RwSignal<Language>,
) -> Memo<Vec<CommentLines>> {
    Memo::new(move |_| {
        text.with(|text| {
            let behavior = edit_language.get().behavior();
            let comments = behavior.comments(text);
            let strings = behavior.multiline_strings(text);
            super::highlight::comment_lines::spanning_lines(text, &comments, &strings)
        })
    })
}

impl super::state::EditorState {
    /// Whether UTF-16 `offset` of the document is on a line inside a
    /// multi-line string's body. Read off the view's memo, which the code
    /// layer keeps anyway: the keys asking on every keystroke never parse
    /// the whole document again.
    pub(super) fn in_string_body(&self, offset: usize) -> bool {
        let line = self.doc().text.with_untracked(|text| {
            let byte = super::keyboard::utf16_to_byte_index(text, offset);
            line_breaks(&text.as_bytes()[..byte])
        });
        self.view()
            .string_bodies
            .with_untracked(|bodies| bodies.iter().any(|lines| lines.contains(&line)))
    }
}

/// Line breaks in `bytes`.
fn line_breaks(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}
