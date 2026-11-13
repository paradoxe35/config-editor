//! The rows of a long list of equal rows that exist in the DOM: those in
//! view and a screen's worth more each way, with spacers standing in for
//! the rest so the scroll bar and the rows' places stay as if all were
//! there. The code view, the gutter and the suggestion list render through
//! it (each row through a [`KeyedList`](super::KeyedList)).

use std::ops::Range;

use leptos::prelude::*;

/// Rows rendered above and below the visible ones, so fast scrolling does
/// not show blank rows before the next frame: a screen's worth each way,
/// at least `MIN_OVERSCAN_ROWS` and at most `MAX_OVERSCAN_ROWS`.
const MIN_OVERSCAN_ROWS: usize = 10;
const MAX_OVERSCAN_ROWS: usize = 40;

/// The rows to render of `count`, for a view `height` high scrolled `top`
/// past the first row, rows `row_height` high: those in view, and a
/// screen's worth more each way (see `MIN_OVERSCAN_ROWS`).
pub(crate) fn row_window(top: f64, row_height: f64, height: f64, count: usize) -> Range<usize> {
    let row_height = row_height.max(1.0);
    let in_view = rows_in(row_height, height);
    let overscan = in_view.clamp(MIN_OVERSCAN_ROWS, MAX_OVERSCAN_ROWS);
    let first = (top / row_height).floor().max(0.0) as usize;
    let first = first.saturating_sub(overscan).min(count);
    let last = (first + in_view + 2 * overscan).min(count);
    first..last
}

/// The most rows [`row_window`] renders for a view `height` high.
pub(crate) fn most_rows(row_height: f64, height: f64) -> usize {
    let in_view = rows_in(row_height.max(1.0), height);
    in_view + 2 * in_view.clamp(MIN_OVERSCAN_ROWS, MAX_OVERSCAN_ROWS)
}

/// The rows a view `height` high shows, part rows counted.
fn rows_in(row_height: f64, height: f64) -> usize {
    (height.max(0.0) / row_height).ceil() as usize
}

/// The room of `rows` rows not rendered, each `row_height` high (a CSS
/// length: `1lh`, `44px`), keeping the rendered rows at their places.
#[component]
pub(crate) fn RowSpacer(
    rows: impl Fn() -> usize + Send + 'static,
    row_height: impl Fn() -> &'static str + Send + 'static,
) -> impl IntoView {
    view! {
        <span
            class="config-editor__row-spacer"
            aria-hidden="true"
            style:height=move || format!("calc({} * {})", rows(), row_height())
        ></span>
    }
}

#[cfg(test)]
mod tests {
    use super::{most_rows, row_window};

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
        // Never past the end.
        assert_eq!(row_window(0.0, 20.0, 200.0, 12), 0..12);
        assert_eq!(row_window(1e9, 20.0, 200.0, 12), 12..12);
    }

    #[test]
    fn the_most_rows_is_what_a_window_renders_away_from_the_ends() {
        for (row_height, height) in [(20.0, 200.0), (20.0, 40.0), (20.0, 2000.0), (44.0, 320.0)] {
            assert_eq!(
                most_rows(row_height, height),
                row_window(50_000.0, row_height, height, 100_000).len(),
                "{row_height} px rows, {height} px high"
            );
        }
    }
}
