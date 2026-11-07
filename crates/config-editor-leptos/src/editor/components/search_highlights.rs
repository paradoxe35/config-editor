//! Boxes behind the find widget's matches, the current one distinct. Only
//! matches on rendered rows are drawn, so a document with thousands of
//! matches costs no more than one with a screenful.

use std::ops::Range;

use leptos::prelude::*;

use super::super::completion::visual_column;
use super::super::search::SearchMatch;
use super::super::state::EditorState;

/// Most boxes drawn at once (a minified line can hold thousands of matches).
const MAX_BOXES: usize = 1_000;

/// A box: zero-based line, left column and width in rendered columns.
#[derive(Clone, Copy, PartialEq)]
struct MatchBox {
    line: usize,
    left: f64,
    width: f64,
    current: bool,
}

#[component]
pub(crate) fn SearchHighlights(state: EditorState) -> impl IntoView {
    view! {
        <div
            class="config-editor__search-highlights"
            aria-hidden="true"
            style:transform=move || format!("translate({}px, {}px)", -state.scroll_x.get(), -state.scroll_y.get())
        >
            {move || boxes(state).into_iter().map(match_box).collect_view()}
        </div>
    }
}

fn match_box(found: MatchBox) -> impl IntoView {
    view! {
        <span
            class="config-editor__search-match"
            class:config-editor__search-match-current=found.current
            style=format!(
                "left: {}ch; top: calc(var(--config-editor-padding-top) + {} * var(--config-editor-line-height)); width: {}ch;",
                found.left,
                found.line,
                found.width,
            )
        ></span>
    }
}

/// The boxes for the matches on the rendered rows.
fn boxes(state: EditorState) -> Vec<MatchBox> {
    let search = state.search;
    let rows = state.visible_rows.get();
    let current = search.current.get();
    search.results.with(|results| {
        if results.matches.is_empty() {
            return Vec::new();
        }
        state.source.with(|text| {
            // A match starting above the rows may run into them.
            let first = results
                .matches
                .partition_point(|found| found.line < rows.start)
                .saturating_sub(1);
            let mut boxes = Vec::new();
            for (index, found) in results.matches.iter().enumerate().skip(first) {
                if found.line >= rows.end || boxes.len() >= MAX_BOXES {
                    break;
                }
                push_segments(text, found, Some(index) == current, &rows, &mut boxes);
            }
            boxes
        })
    })
}

/// One box per line `found` covers within `rows`.
fn push_segments(
    text: &str,
    found: &SearchMatch,
    current: bool,
    rows: &Range<usize>,
    boxes: &mut Vec<MatchBox>,
) {
    let (mut line, mut line_start, mut start) = (found.line, found.line_start, found.start);
    loop {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |newline| line_start + newline);
        let end = found.end.min(line_end);
        if rows.contains(&line) && start <= end {
            let left = visual_column(&text[line_start..start]);
            let width = visual_column(&text[line_start..end]) - left;
            boxes.push(MatchBox {
                line,
                left,
                // A match ending in a line break still shows on its line.
                width: width.max(0.5),
                current,
            });
        }
        if found.end <= line_end + 1 || line + 1 >= rows.end {
            return;
        }
        line += 1;
        line_start = line_end + 1;
        start = line_start;
    }
}
