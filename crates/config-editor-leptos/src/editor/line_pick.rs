//! Selecting whole lines from the line numbers, as in VS Code: a press on a
//! number selects its line (a closed fold's line with the lines it hides), a
//! drag over the numbers selects every line it passes, and `Shift` with a
//! press extends the selection by whole lines to the number pressed.
//! Held past the text's top or bottom, the drag scrolls the text on and
//! selects the lines it brings into view, as a drag over the text does.
//!
//! The mouse only: a finger on the numbers scrolls the text (see
//! `scroll_through`), and its tap selects the line through the click.

use leptos::prelude::*;

use super::metrics::read_editor_metrics;
use super::paths::line_start;
use super::projection::Projection;
use super::source_area::SourceArea;
use super::state::{EditorState, directed_selection};
use super::text::TextSize;
use crate::reactive::later;

/// How often a drag held past the text scrolls it, in milliseconds.
const SCROLL_EVERY_MS: i32 = 40;

/// Most lines a drag held past the text scrolls at a time.
const MOST_LINES_PER_STEP: f64 = 8.0;

/// One gutter's line picking: what the press started, for the drag and the
/// next `Shift` press.
#[derive(Clone, Copy)]
pub(super) struct LinePick {
    state: StoredValue<Picked>,
    /// The rows' container, which holds the pointer while dragging.
    rows: NodeRef<leptos::html::Div>,
}

#[derive(Clone, Copy, Default)]
struct Picked {
    /// The lines the selection grows from, as bytes, and the selection
    /// last made from them: a `Shift` press grows from them while the
    /// selection is still that one.
    anchor: Option<(TextSize, TextSize)>,
    made: Option<(TextSize, TextSize)>,
    /// A mouse button is held since a press on a number.
    dragging: bool,
    /// The drag left the row pressed and holds the pointer.
    captured: bool,
    /// The pointer's last window y while dragging.
    pointer_y: f64,
    /// A step of the scroll past the text's edge is waiting.
    scrolling: bool,
    /// The last press on a number was the mouse's, which selected already:
    /// the click after it does nothing more.
    by_mouse: bool,
}

impl LinePick {
    pub(super) fn new(rows: NodeRef<leptos::html::Div>) -> Self {
        Self {
            state: StoredValue::new(Picked::default()),
            rows,
        }
    }

    /// A press on the number of document line `line` (0-based).
    pub(super) fn press(self, state: EditorState, line: usize, event: &web_sys::PointerEvent) {
        let by_mouse = event.pointer_type() == "mouse" && event.button() == 0;
        self.state.update_value(|picked| picked.by_mouse = by_mouse);
        if !by_mouse {
            return;
        }
        let Some(area) = state.textarea() else {
            return;
        };
        let row = area.row_of_line(line);
        let lines = state.doc().text.with_untracked(|text| {
            state
                .view()
                .projection
                .with_untracked(|projection| row_bytes(text, projection, row))
        });
        let picked = self.state.get_value();
        let anchor = if event.shift_key() {
            let (anchor, caret) = directed_selection(&area);
            let now = (anchor.min(caret), anchor.max(caret));
            picked
                .anchor
                .filter(|_| picked.made == Some(now))
                .unwrap_or((anchor, anchor))
        } else {
            lines
        };
        self.state.update_value(|picked| {
            picked.anchor = Some(anchor);
            picked.dragging = true;
        });
        self.select(state, anchor, lines);
    }

    /// The pointer moved over the gutter: while a press on a number is
    /// held, the lines from it to the row under the pointer are selected.
    pub(super) fn drag(self, state: EditorState, event: &web_sys::PointerEvent) {
        let picked = self.state.get_value();
        if !picked.dragging {
            return;
        }
        let (true, Some(anchor), Some(area)) =
            (event.buttons() & 1 == 1, picked.anchor, state.textarea())
        else {
            return;
        };
        let y = f64::from(event.client_y());
        self.state.update_value(|picked| picked.pointer_y = y);
        let lines = lines_at(state, &area, y);
        let (start, end, _) = spanned(anchor, lines);
        if picked.made != Some((start, end)) {
            // Off the row pressed the drag holds the pointer (see `hold`).
            self.hold(event);
            self.extend(&area, anchor, lines);
        }
        self.scroll_past_edge(state, &area);
    }

    /// While the drag is held past the text's top or bottom: a step of
    /// scrolling waits, unless one already does.
    fn scroll_past_edge(self, state: EditorState, area: &SourceArea) {
        let picked = self.state.get_value();
        if !picked.dragging || picked.scrolling || past_edge(area, picked.pointer_y) == 0.0 {
            return;
        }
        self.state.update_value(|picked| picked.scrolling = true);
        later::after(self.state, SCROLL_EVERY_MS, move |_| {
            self.scroll_on(state);
        });
    }

    /// One step of the scroll past the text's edge: the text scrolls by
    /// more lines the further past the edge the pointer is, and the lines
    /// to the row at that edge are selected. It goes on until the pointer
    /// is back over the text, the text can scroll no further, or the
    /// button is let go.
    fn scroll_on(self, state: EditorState) {
        self.state.update_value(|picked| picked.scrolling = false);
        let picked = self.state.get_value();
        let (true, Some(anchor), Some(area)) = (picked.dragging, picked.anchor, state.textarea())
        else {
            return;
        };
        let past = past_edge(&area, picked.pointer_y);
        if past == 0.0 {
            return;
        }
        let line_height = read_editor_metrics(area.element()).line_height.max(1.0);
        let before = area.scroll_top();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a scroll step of a few lines' pixels"
        )]
        let step = scroll_step(past, line_height).round() as i32;
        area.set_scroll_top(before.saturating_add(step));
        self.extend(&area, anchor, lines_at(state, &area, picked.pointer_y));
        if area.scroll_top() != before {
            self.scroll_past_edge(state, &area);
        }
    }

    /// The pointer left the row pressed, or the gutter, while dragging: from
    /// then on its moves go to the rows (and from them to the gutter) until
    /// the button is let go, also over the text and when the row pressed is
    /// scrolled out and dropped. Not at the press: the click of a press that
    /// stays on its row keeps its target (a change marker opens its change).
    pub(super) fn hold(self, event: &web_sys::PointerEvent) {
        let picked = self.state.get_value();
        if !picked.dragging || picked.captured || event.buttons() & 1 != 1 {
            return;
        }
        if let Some(rows) = self.rows.get_untracked() {
            let _ = rows.set_pointer_capture(event.pointer_id());
            self.state.update_value(|picked| picked.captured = true);
        }
    }

    /// The button is let go (or the pointer lost): the drag ends.
    pub(super) fn release(self) {
        self.state.update_value(|picked| {
            picked.dragging = false;
            picked.captured = false;
            picked.scrolling = false;
        });
    }

    /// Whether the click after a press is the press's own, which selected
    /// already (a mouse's); a tap's click selects its line itself.
    pub(super) fn clicked(self) -> bool {
        let by_mouse = self.state.get_value().by_mouse;
        self.state.update_value(|picked| picked.by_mouse = false);
        by_mouse
    }

    fn select(self, state: EditorState, anchor: (TextSize, TextSize), lines: (TextSize, TextSize)) {
        let (start, end, backward) = spanned(anchor, lines);
        if state.focus_range_directed(start, end, backward).is_some() {
            self.state
                .update_value(|picked| picked.made = Some((start, end)));
        }
    }

    /// The drag reached `lines`: the selection grows to them, the text
    /// kept where the drag scrolled it (the press revealed and focused;
    /// revealing the selection's start again would scroll a drag past the
    /// text back to it).
    fn extend(self, area: &SourceArea, anchor: (TextSize, TextSize), lines: (TextSize, TextSize)) {
        let (start, end, backward) = spanned(anchor, lines);
        let (top, left) = (area.scroll_top(), area.scroll_left());
        let direction = if backward { "backward" } else { "forward" };
        if area
            .set_selection_range_with_direction(start, end, direction)
            .is_ok()
        {
            self.state
                .update_value(|picked| picked.made = Some((start, end)));
        }
        area.set_scroll_top(top);
        area.set_scroll_left(left);
    }
}

/// The lines of the textarea row at window y `y`, kept to the rows in the
/// text area's view (past its top or bottom, the row at that edge).
fn lines_at(state: EditorState, area: &SourceArea, y: f64) -> (TextSize, TextSize) {
    let element = area.element();
    let rect = element.get_bounding_client_rect();
    let metrics = read_editor_metrics(element);
    let line_height = metrics.line_height.max(1.0);
    let y = y.clamp(rect.top(), (rect.bottom() - 1.0).max(rect.top()));
    let row = (y - rect.top() - metrics.padding_top + f64::from(area.scroll_top())) / line_height;
    state.doc().text.with_untracked(|text| {
        state.view().projection.with_untracked(|projection| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a row from a pixel offset, clamped to the rows"
            )]
            let row = row
                .floor()
                .clamp(0.0, projection.rows().saturating_sub(1) as f64)
                as usize;
            row_bytes(text, projection, row)
        })
    })
}

/// How far window y `y` is past the text area's top (below zero) or
/// bottom (above zero), in pixels; zero over it.
fn past_edge(area: &SourceArea, y: f64) -> f64 {
    let rect = area.element().get_bounding_client_rect();
    if y < rect.top() {
        y - rect.top()
    } else if y > rect.bottom() {
        y - rect.bottom()
    } else {
        0.0
    }
}

/// The pixels a drag `past` pixels past the text's edge scrolls it by in
/// a step: a line, and one more for each line's height further, at most
/// [`MOST_LINES_PER_STEP`]; up when `past` is below zero.
fn scroll_step(past: f64, line_height: f64) -> f64 {
    let lines = (past.abs() / line_height).floor() + 1.0;
    lines.min(MOST_LINES_PER_STEP) * line_height * past.signum()
}

/// The bytes of textarea row `row` of `text` shown through `projection`:
/// from its line's start to the start of the next row's line (the end of
/// the text after the last), so a closed fold's row holds the lines it
/// hides.
fn row_bytes(text: &str, projection: &Projection, row: usize) -> (TextSize, TextSize) {
    let start = line_start(text, projection.line_of_row(row));
    let end = if row + 1 < projection.rows() {
        line_start(text, projection.line_of_row(row + 1))
    } else {
        TextSize::of(text)
    };
    (start, end)
}

/// The selection over the lines `anchor` and `lines` and every line
/// between, as `(start, end, backward)`: backward (the caret at its start)
/// when `lines` begin before the anchor.
fn spanned(
    anchor: (TextSize, TextSize),
    lines: (TextSize, TextSize),
) -> (TextSize, TextSize, bool) {
    (
        anchor.0.min(lines.0),
        anchor.1.max(lines.1),
        lines.0 < anchor.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::fold_ranges::FoldRange;

    fn bytes(text: &str, folds: &[FoldRange], row: usize) -> (usize, usize) {
        let (start, end) = row_bytes(text, &Projection::new(text, folds), row);
        (usize::from(start), usize::from(end))
    }

    #[test]
    fn a_row_holds_its_line_and_its_break_and_a_closed_fold_its_hidden_lines() {
        let text = "a:\n  b: 1\n  c: 2\nd: 3";
        assert_eq!(bytes(text, &[], 0), (0, 3));
        assert_eq!(bytes(text, &[], 1), (3, 10));
        assert_eq!(bytes(text, &[], 3), (17, 21));
        let folded = [FoldRange::new(0, 2)];
        assert_eq!(bytes(text, &folded, 0), (0, 17));
        assert_eq!(bytes(text, &folded, 1), (17, 21));
    }

    /// Held past the text, a drag scrolls a line a step, faster the
    /// further past, up above the text and down below it.
    #[test]
    fn a_drag_past_the_text_scrolls_faster_the_further_past() {
        assert_eq!(scroll_step(1.0, 20.0), 20.0);
        assert_eq!(scroll_step(-1.0, 20.0), -20.0);
        assert_eq!(scroll_step(45.0, 20.0), 60.0);
        assert_eq!(scroll_step(-45.0, 20.0), -60.0);
        assert_eq!(scroll_step(10_000.0, 20.0), MOST_LINES_PER_STEP * 20.0);
    }

    #[test]
    fn the_lines_between_the_anchor_and_the_pointer_are_selected_toward_the_pointer() {
        let lines = |start: u32, end: u32| (TextSize::from(start), TextSize::from(end));
        // Down from the anchor: forward, to the end of the line reached.
        assert_eq!(
            spanned(lines(3, 10), lines(17, 21)),
            (3.into(), 21.into(), false)
        );
        // Up: backward, the caret at the start of the line reached.
        assert_eq!(
            spanned(lines(10, 17), lines(0, 3)),
            (0.into(), 17.into(), true)
        );
        // Back on the anchor: its line alone.
        assert_eq!(
            spanned(lines(3, 10), lines(3, 10)),
            (3.into(), 10.into(), false)
        );
        // From a caret (a Shift press): to the whole line pressed.
        assert_eq!(
            spanned(lines(5, 5), lines(10, 17)),
            (5.into(), 17.into(), false)
        );
        assert_eq!(
            spanned(lines(12, 12), lines(3, 10)),
            (3.into(), 12.into(), true)
        );
    }
}
