//! What the view is laid out from: the elements, the scroll position, the
//! lines as rendered, and which textarea rows exist in the DOM.

use std::ops::Range;
use std::sync::Arc;

use leptos::prelude::*;

use super::highlight::spanning::Spanning;
use super::languages::Language;
use super::line_rows::RenderedLine;
use super::metrics::read_editor_metrics;
use super::projection::Projection;
use super::readings::Readings;

/// Rows rendered above and below the visible ones, so fast scrolling does
/// not show blank lines before the next frame: a screen's worth each way,
/// at least `MIN_OVERSCAN_ROWS` and at most `MAX_OVERSCAN_ROWS`.
const MIN_OVERSCAN_ROWS: usize = 10;
const MAX_OVERSCAN_ROWS: usize = 40;

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
    /// The probe the popups' frame is measured from (see `PopupFrame`).
    pub(super) frame_probe_ref: NodeRef<leptos::html::Div>,
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
    /// The comments, strings and structure written over several lines.
    pub(super) spanning: Memo<Spanning>,
    /// The textarea's text: the document with the closed folds cut out
    /// (see `projection`; the document itself without folding).
    pub(super) projection: RwSignal<Arc<Projection>>,
    /// The document lines (0-based) on the textarea rows in view.
    pub(super) visible_lines: Memo<Vec<usize>>,
    /// Where the last page key left the caret, and the x it keeps
    /// going for, in the text area's pixels (see `keys::page_keys`).
    pub(super) page_goal: StoredValue<Option<(usize, f64)>>,
}

impl ViewState {
    /// The view of the document, whose textarea shows `projection`, its lines
    /// rendered as `lines`, with the multi-line string bodies of
    /// [`string_bodies`] and the constructs over several lines of
    /// [`spanning`].
    pub(super) fn new(
        projection: RwSignal<Arc<Projection>>,
        lines: Memo<Vec<RenderedLine>>,
        string_bodies: Memo<Vec<Range<usize>>>,
        spanning: Memo<Spanning>,
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
            // The text area's own height; the window's before it has one
            // (its resize bumps `layout_revision`).
            let height = match editor.client_height() {
                0 => web_sys::window()
                    .and_then(|window| window.inner_height().ok())
                    .and_then(|height| height.as_f64())
                    .unwrap_or(0.0),
                height => f64::from(height),
            };
            row_window(
                top - metrics.padding_top,
                metrics.line_height,
                height,
                count,
            )
        });
        let visible_lines = Memo::new(move |_| {
            let rows = visible_rows.get();
            projection.with(|projection| rows.map(|row| projection.line_of_row(row)).collect())
        });
        Self {
            textarea_ref,
            completion_ref: NodeRef::new(),
            root_ref: NodeRef::new(),
            frame_probe_ref: NodeRef::new(),
            scroll_x: RwSignal::new(0.0),
            scroll_y,
            layout_revision,
            lines,
            visible_rows,
            string_bodies,
            spanning,
            projection,
            visible_lines,
            page_goal: StoredValue::new(None),
        }
    }
}

/// The rows to render of `count`, for a view `height` high scrolled `top`
/// past the first row, rows `line_height` high: those in view, and a
/// screen's worth more each way (see `MIN_OVERSCAN_ROWS`).
fn row_window(top: f64, line_height: f64, height: f64, count: usize) -> Range<usize> {
    let line_height = line_height.max(1.0);
    let in_view = (height.max(0.0) / line_height).ceil() as usize;
    let overscan = in_view.clamp(MIN_OVERSCAN_ROWS, MAX_OVERSCAN_ROWS);
    let first = (top / line_height).floor().max(0.0) as usize;
    let first = first.saturating_sub(overscan).min(count);
    let last = (first + in_view + 2 * overscan).min(count);
    first..last
}

/// The lines inside multi-line strings' bodies of `text` (in
/// `edit_language`), found once per edit for the view and the suggestions.
pub(super) fn string_bodies(
    text: RwSignal<String>,
    edit_language: RwSignal<Language>,
    readings: Readings,
) -> Memo<Vec<Range<usize>>> {
    readings.memo(text, move || {
        text.with(|text| {
            super::parse_cache::string_bodies(edit_language.get().behavior(), text).to_vec()
        })
    })
}

/// The comments, strings and structure `text` (in `edit_language`) writes
/// over several lines, found once per edit for the code layer.
pub(super) fn spanning(
    text: RwSignal<String>,
    edit_language: RwSignal<Language>,
    readings: Readings,
) -> Memo<Spanning> {
    readings.memo(text, move || {
        super::work_count::note(super::work_count::Work::Spanning);
        text.with(|text| {
            let behavior = edit_language.get().behavior();
            Spanning::of(
                text,
                &behavior.comments(text),
                &behavior.multiline_constructs(text),
            )
        })
    })
}

impl super::state::EditorState {
    /// Whether UTF-16 `offset` of the document is on a line inside a
    /// multi-line string's body. Read off the view's memo, which the code
    /// layer keeps anyway: the keys asking on every keystroke never parse
    /// the whole document again.
    pub(super) fn in_string_body(&self, offset: usize) -> bool {
        self.settle_readings();
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

#[cfg(test)]
mod tests {
    use super::row_window;

    #[test]
    fn a_small_view_renders_a_screen_more_each_way_not_a_fixed_band() {
        // A 200 px editor with 20 px rows shows 10: 30 rendered, not 90.
        assert_eq!(row_window(0.0, 20.0, 200.0, 10_000), 0..30);
        assert_eq!(row_window(20_000.0, 20.0, 200.0, 10_000), 990..1020);
        // A tiny one keeps the minimum band for fast scrolls.
        assert_eq!(row_window(20_000.0, 20.0, 40.0, 10_000).len(), 2 + 2 * 10);
        // A tall one is capped.
        assert_eq!(
            row_window(20_000.0, 20.0, 2000.0, 10_000).len(),
            100 + 2 * 40
        );
        // Never past the document.
        assert_eq!(row_window(0.0, 20.0, 200.0, 12), 0..12);
        assert_eq!(row_window(1e9, 20.0, 200.0, 12), 12..12);
    }
}
