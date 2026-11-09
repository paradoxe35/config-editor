//! `PageDown` / `PageUp` in the text page the editor only. The browser's
//! own page keys move the caret and scroll the text, but Chrome also
//! scrolls every scroller around the text area (the host page too) to put
//! the caret at their top, so the editor makes the move itself: the caret
//! goes a page of rows down (or up), at the on-screen x it paged from (as
//! `ArrowDown` keeps it, measured where the text is drawn: see
//! [`page_ruler`](super::page_ruler)), and the text scrolls by the same
//! page. A page
//! is the rows in view: of the text area, or of the window when the text
//! area is taller than it. Outside the editor, nothing moves but what
//! brings the caret into view, as for `ArrowDown` (see
//! [`crate::page_scroll`] for the keys from its other controls).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::keyboard::utf16_to_byte_index;
use super::super::metrics::read_editor_metrics;
use super::super::positioning::reveal_offset;
use super::super::projection::Projection;
use super::super::source_area::SourceArea;
use super::super::state::{EditorState, directed_selection, select};
use super::page_ruler::Ruler;
use crate::page_scroll::{band_in_view, reveal_band, scale_of};

/// Pages the text on `PageDown` / `PageUp` (with Shift, selecting).
/// Returns whether it took the key.
pub(super) fn page(state: EditorState, event: &KeyboardEvent) -> bool {
    let down = match event.key().as_str() {
        "PageDown" => true,
        "PageUp" => false,
        _ => return false,
    };
    if event.alt_key() || event.ctrl_key() || event.meta_key() {
        return false;
    }
    let Some(area) = state.textarea() else {
        return false;
    };
    event.prevent_default();
    let metrics = read_editor_metrics(area.element());
    let line_height = metrics.line_height.max(1.0);
    let rows = page_rows(rows_height(&area), line_height);
    let (anchor, caret) = directed_selection(&area);
    let target = state.doc().text.with_untracked(|text| {
        let ruler = Ruler::new(&area, metrics.char_width);
        let goal = state
            .view()
            .page_goal
            .get_value()
            .filter(|(at, _)| *at == caret)
            .map_or_else(
                || {
                    let (line, units) = line_at(text, caret);
                    ruler.goal(line, units)
                },
                |(_, goal)| goal,
            );
        let target = state.view().projection.with_untracked(|projection| {
            paged(text, projection, caret, rows, down, |line| {
                ruler.nearest(line, goal)
            })
        });
        state.view().page_goal.set_value(Some((target, goal)));
        target
    });
    if event.shift_key() {
        select(
            &area,
            anchor.min(target),
            anchor.max(target),
            target < anchor,
        );
    } else {
        let _ = area.set_selection_range(target as u32, target as u32);
    }
    let step = (rows as f64 * line_height).round() as i32;
    let top = area.scroll_top();
    area.set_scroll_top(if down { top + step } else { top - step }.max(0));
    state.view().scroll_y.set(f64::from(area.scroll_top()));
    state
        .doc()
        .text
        .with_untracked(|text| reveal_offset(&area, text, target, state.view().scroll_y));
    reveal_in_page(&area, target, metrics.padding_top, line_height);
    state.note_caret(target);
    true
}

/// The height, in the text area's pixels, its rows show in: the part of
/// it in view (inside the window and the boxes around it), or, when none
/// is, its own height, no taller than the window.
fn rows_height(area: &SourceArea) -> f64 {
    let element: &web_sys::Element = area.element();
    let scale = scale_of(element).max(f64::EPSILON);
    let own = f64::from(area.client_height());
    match band_in_view(element) {
        Some((top, bottom)) => ((bottom - top) / scale).min(own),
        None => web_sys::window()
            .and_then(|window| window.inner_height().ok())
            .and_then(|height| height.as_f64())
            .map_or(own, |height| own.min(height / scale)),
    }
}

/// Shows the caret's row, at document offset `target`, in the boxes
/// around the text area (a text area taller than the window does not
/// scroll itself): they move only as far as that takes.
fn reveal_in_page(area: &SourceArea, target: usize, padding_top: f64, line_height: f64) {
    let line = area.state().doc().text.with_untracked(|text| {
        let byte = utf16_to_byte_index(text, target);
        text[..byte].bytes().filter(|byte| *byte == b'\n').count()
    });
    let row = area.row_of_line(line) as f64;
    let element: &web_sys::Element = area.element();
    let scale = scale_of(element);
    let rect = element.get_bounding_client_rect();
    let top = rect.top()
        + (f64::from(element.client_top()) + padding_top + row * line_height
            - f64::from(area.scroll_top()))
            * scale;
    reveal_band(element, top, top + line_height * scale);
}

/// The rows a page key moves in a text area `height` pixels tall: all
/// those in view but one, so a row of the last page stays in sight.
fn page_rows(height: f64, line_height: f64) -> usize {
    ((height / line_height).floor() as usize)
        .saturating_sub(1)
        .max(1)
}

/// The line of document offset `offset` (UTF-16), and how far into it
/// the offset is.
fn line_at(text: &str, offset: usize) -> (&str, usize) {
    let byte = utf16_to_byte_index(text, offset);
    let start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
    let end = text[byte..].find('\n').map_or(text.len(), |at| byte + at);
    (&text[start..end], text[start..byte].encode_utf16().count())
}

/// Where a page key takes the caret at document offset `caret` (UTF-16):
/// `rows` textarea rows down (or up), where `land` puts it on that row's
/// line (a UTF-16 offset into it); from the last (first) row, the end
/// (start) of the text. Folded lines are no rows.
fn paged(
    text: &str,
    projection: &Projection,
    caret: usize,
    rows: usize,
    down: bool,
    land: impl FnOnce(&str) -> usize,
) -> usize {
    // Each line's start, as a UTF-16 offset, and its text.
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut units = 0;
    for line in text.split('\n') {
        lines.push((units, line));
        units += line.encode_utf16().count() + 1;
    }
    let units = units - 1;
    let line = lines
        .partition_point(|(line_start, _)| *line_start <= caret)
        .saturating_sub(1);
    let row = projection.row_of_line(line);
    let last_row = projection.rows().saturating_sub(1);
    let target_row = if down {
        (row + rows).min(last_row)
    } else {
        row.saturating_sub(rows)
    };
    if target_row == row {
        return if down { units } else { 0 };
    }
    let (line_start, line) = lines[projection.line_of_row(target_row).min(lines.len() - 1)];
    line_start + land(line)
}

#[cfg(test)]
mod tests {
    use super::super::super::fold_ranges::FoldRange;
    use super::*;

    /// Where a page key takes the caret, for on-screen column `goal`,
    /// where nothing is drawn to measure (see `page_ruler`).
    fn by_columns(
        text: &str,
        projection: &Projection,
        caret: usize,
        goal: usize,
        rows: usize,
        down: bool,
    ) -> usize {
        paged(text, projection, caret, rows, down, |line| {
            COLUMNS.nearest(line, goal as f64)
        })
    }

    const COLUMNS: Ruler = Ruler::Columns { width: 1.0 };

    fn text() -> String {
        (0..20).map(|line| format!("line {line}\n")).collect()
    }

    /// The UTF-16 offset of `column` on zero-based `line` of `text`.
    fn at(text: &str, line: usize, column: usize) -> usize {
        text.split('\n')
            .take(line)
            .map(|line| line.encode_utf16().count() + 1)
            .sum::<usize>()
            + column
    }

    #[test]
    fn a_page_moves_the_caret_by_rows_keeping_its_column() {
        let text = text();
        let flat = Projection::new(&text, &[]);
        let caret = at(&text, 2, 3);
        assert_eq!(by_columns(&text, &flat, caret, 3, 5, true), at(&text, 7, 3));
        assert_eq!(
            by_columns(&text, &flat, at(&text, 7, 3), 3, 5, false),
            caret
        );
        // Near the end, to the last row; from it, to the text's end.
        assert_eq!(
            by_columns(&text, &flat, at(&text, 18, 3), 3, 5, true),
            at(&text, 20, 0)
        );
        assert_eq!(
            by_columns(&text, &flat, at(&text, 20, 0), 3, 5, true),
            text.encode_utf16().count()
        );
        // From the first row, to the start.
        assert_eq!(by_columns(&text, &flat, at(&text, 0, 4), 4, 5, false), 0);
    }

    #[test]
    fn the_goal_column_is_kept_past_short_lines() {
        let text = "a long line\nab\nshort\nanother long line\n";
        let flat = Projection::new(text, &[]);
        // Onto a short line: at its end.
        assert_eq!(by_columns(text, &flat, 9, 9, 1, true), at(text, 1, 2));
        // From there, still for column 9.
        assert_eq!(
            by_columns(text, &flat, at(text, 1, 2), 9, 2, true),
            at(text, 3, 9)
        );
    }

    #[test]
    fn folded_lines_are_no_rows() {
        let text = text();
        let folded = Projection::new(&text, &[FoldRange { start: 3, end: 10 }]);
        // Rows: 0 1 2 3(4..10 hidden) 11 12 …; five rows down from line 1
        // is line 13.
        assert_eq!(
            by_columns(&text, &folded, at(&text, 1, 0), 0, 5, true),
            at(&text, 13, 0)
        );
        assert_eq!(
            by_columns(&text, &folded, at(&text, 13, 0), 0, 5, false),
            at(&text, 1, 0)
        );
    }

    /// QA 26: the column was counted in UTF-16 units, so a page key from
    /// after an emoji or a tab landed right or left of where `ArrowDown`
    /// does. Every line pair pages from its first line's end onto the
    /// on-screen column below it.
    #[test]
    fn a_page_keeps_the_on_screen_column() {
        // (line paged from, its end's column, line paged to, where it lands)
        let cases = [
            ("a: 😀😀😀", 6, "k: yyyyyyyy", "k: yyy"),
            ("a:\t\tx", 7, "k: yyyyyyyy", "k: yyyy"),
            ("a: 漢字漢字", 7, "k: yyyyyyyy", "k: yyyy"),
            ("a: e\u{301}e\u{301}", 5, "k: yyyyyyyy", "k: yy"),
            ("a: yyyy", 7, "k:\t\t\tz", "k:\t\t\t"),
            ("a: yyyyyy", 9, "k: 😀😀😀😀", "k: 😀😀😀😀"),
            (
                "a: yy",
                5,
                "k: e\u{301}e\u{301}e\u{301}",
                "k: e\u{301}e\u{301}",
            ),
            ("a: yy", 5, "k: 漢字漢字", "k: 漢字"),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter_map(|(from, column, to, landed)| {
                let text = format!("{from}\n{to}\n");
                let flat = Projection::new(&text, &[]);
                let end = from.encode_utf16().count();
                let (line, units) = line_at(&text, end);
                let goal = COLUMNS.goal(line, units) as usize;
                let target = by_columns(&text, &flat, end, goal, 1, true);
                let expected = at(&text, 1, landed.encode_utf16().count());
                (goal != *column || target != expected).then(|| {
                    format!("{from:?} -> {to:?}: column {goal} (not {column}), at {target} (not {expected})")
                })
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn a_page_is_the_rows_in_view_but_one() {
        assert_eq!(page_rows(400.0, 20.0), 19);
        assert_eq!(page_rows(10.0, 20.0), 1);
        assert_eq!(page_rows(41.0, 20.0), 1);
    }
}
