//! Selecting whole lines from the line numbers, as in VS Code: a press on a
//! number selects its line (a closed fold's line with the lines it hides), a
//! drag over the numbers selects every line it passes, and `Shift` with a
//! press extends the selection by whole lines to the number pressed.
//!
//! The mouse only: a finger on the numbers scrolls the text (see
//! `scroll_through`), and its tap selects the line through the click.

use leptos::prelude::*;

use super::metrics::read_editor_metrics;
use super::paths::line_start;
use super::projection::Projection;
use super::state::{EditorState, directed_selection};
use super::text::TextSize;

/// One gutter's line picking: what the press started, for the drag and the
/// next `Shift` press.
#[derive(Clone, Copy)]
pub(super) struct LinePick {
    state: StoredValue<Picked>,
    /// The rows' container, whose top is row 0's.
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
        let (true, Some(anchor), Some(rows), Some(area)) = (
            event.buttons() & 1 == 1,
            picked.anchor,
            self.rows.get_untracked(),
            state.textarea(),
        ) else {
            return;
        };
        let line_height = read_editor_metrics(area.element()).line_height.max(1.0);
        let top = rows.get_bounding_client_rect().top();
        let lines = state.doc().text.with_untracked(|text| {
            state.view().projection.with_untracked(|projection| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a row from a pixel offset, clamped to the rows"
                )]
                let row = ((f64::from(event.client_y()) - top) / line_height)
                    .floor()
                    .clamp(0.0, projection.rows().saturating_sub(1) as f64)
                    as usize;
                row_bytes(text, projection, row)
            })
        });
        let (start, end, _) = spanned(anchor, lines);
        if picked.made == Some((start, end)) {
            return;
        }
        // Off the row pressed the drag holds the pointer (see `hold`).
        self.hold(event);
        self.select(state, anchor, lines);
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
