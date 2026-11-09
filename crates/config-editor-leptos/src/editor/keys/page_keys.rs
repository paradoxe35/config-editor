//! `PageDown` / `PageUp` in the text page the editor only. The browser's
//! own page keys move the caret and scroll the text, but Chrome also
//! scrolls every scroller around the text area (the host page too) to put
//! the caret at their top, so the editor makes the move itself: the caret
//! goes a page of rows down (or up), at the column it paged from, and the
//! text scrolls by the same page. Nothing outside the editor moves (see
//! [`crate::page_scroll`] for the keys from its other controls).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::keyboard::utf16_to_byte_index;
use super::super::metrics::read_editor_metrics;
use super::super::positioning::reveal_offset;
use super::super::projection::Projection;
use super::super::state::{EditorState, directed_selection, select};

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
    let rows = page_rows(f64::from(area.client_height()), line_height);
    let (anchor, caret) = directed_selection(&area);
    let target = state.doc().text.with_untracked(|text| {
        let goal = state
            .view()
            .page_goal
            .get_value()
            .filter(|(at, _)| *at == caret)
            .map_or_else(|| column_of(text, caret), |(_, goal)| goal);
        let target = state
            .view()
            .projection
            .with_untracked(|projection| paged(text, projection, caret, goal, rows, down));
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
    state.note_caret(target);
    true
}

/// The rows a page key moves in a text area `height` pixels tall: all
/// those in view but one, so a row of the last page stays in sight.
fn page_rows(height: f64, line_height: f64) -> usize {
    ((height / line_height).floor() as usize)
        .saturating_sub(1)
        .max(1)
}

/// The UTF-16 column of document offset `offset` on its line.
fn column_of(text: &str, offset: usize) -> usize {
    let byte = utf16_to_byte_index(text, offset);
    let line_start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
    text[line_start..byte].encode_utf16().count()
}

/// Where a page key takes the caret at document offset `caret` (UTF-16):
/// `rows` textarea rows down (or up), `goal` columns into that row's line
/// (at most its end); from the last (first) row, the end (start) of the
/// text. Folded lines are no rows.
fn paged(
    text: &str,
    projection: &Projection,
    caret: usize,
    goal: usize,
    rows: usize,
    down: bool,
) -> usize {
    // UTF-16 offsets of each line's start, and its length.
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    let mut units = 0;
    for character in text.chars() {
        if character == '\n' {
            lines.push((start, units - start));
            start = units + 1;
        }
        units += character.len_utf16();
    }
    lines.push((start, units - start));
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
    let (line_start, length) = lines[projection.line_of_row(target_row).min(lines.len() - 1)];
    line_start + goal.min(length)
}

#[cfg(test)]
mod tests {
    use super::super::super::fold_ranges::FoldRange;
    use super::*;

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
        assert_eq!(paged(&text, &flat, caret, 3, 5, true), at(&text, 7, 3));
        assert_eq!(paged(&text, &flat, at(&text, 7, 3), 3, 5, false), caret);
        // Near the end, to the last row; from it, to the text's end.
        assert_eq!(
            paged(&text, &flat, at(&text, 18, 3), 3, 5, true),
            at(&text, 20, 0)
        );
        assert_eq!(
            paged(&text, &flat, at(&text, 20, 0), 3, 5, true),
            text.encode_utf16().count()
        );
        // From the first row, to the start.
        assert_eq!(paged(&text, &flat, at(&text, 0, 4), 4, 5, false), 0);
    }

    #[test]
    fn the_goal_column_is_kept_past_short_lines() {
        let text = "a long line\nab\nshort\nanother long line\n";
        let flat = Projection::new(text, &[]);
        // Onto a short line: at its end.
        assert_eq!(paged(text, &flat, 9, 9, 1, true), at(text, 1, 2));
        // From there, still for column 9.
        assert_eq!(
            paged(text, &flat, at(text, 1, 2), 9, 2, true),
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
            paged(&text, &folded, at(&text, 1, 0), 0, 5, true),
            at(&text, 13, 0)
        );
        assert_eq!(
            paged(&text, &folded, at(&text, 13, 0), 0, 5, false),
            at(&text, 1, 0)
        );
    }

    #[test]
    fn non_ascii_columns_count_utf16_units() {
        let text = "é😀x\nab😀cd\n";
        let flat = Projection::new(text, &[]);
        // Column 4 (after the emoji's two units) on the next line.
        assert_eq!(paged(text, &flat, 4, 4, 1, true), at(text, 1, 4));
        assert_eq!(column_of(text, at(text, 1, 4)), 4);
    }

    #[test]
    fn a_page_is_the_rows_in_view_but_one() {
        assert_eq!(page_rows(400.0, 20.0), 19);
        assert_eq!(page_rows(10.0, 20.0), 1);
        assert_eq!(page_rows(41.0, 20.0), 1);
    }
}
