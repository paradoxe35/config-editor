//! Boxes behind the find widget's matches, the current one distinct. Only
//! matches on rendered rows are drawn, so a document with thousands of
//! matches costs no more than one with a screenful.
//!
//! Each row with a match is a copy of its line in the textarea's font, its
//! text invisible and each match a span with the fill, so the browser lays
//! the boxes out as it lays out the text: under the user's letter and word
//! spacing, in right-to-left and mixed-direction runs, over tabs and wide
//! characters, with no arithmetic on columns that has to agree with it.

use std::ops::Range;

use leptos::prelude::*;

use super::super::super::positioning::row_box;
use super::super::super::state::EditorState;
use super::super::SearchMatch;
use crate::cssom::cssom;

/// Most matches drawn at once (a minified line can hold thousands).
const MAX_MATCHES: usize = 1_000;

/// The part of a match on one document line, in byte offsets of the text.
#[derive(Clone, Copy)]
struct Segment {
    line: usize,
    line_start: usize,
    start: usize,
    end: usize,
    current: bool,
}

/// A line with matches: its zero-based textarea row and the pieces of its
/// text, matched or not, in order.
#[derive(Clone, PartialEq)]
struct MirrorRow {
    row: usize,
    pieces: Vec<Piece>,
}

#[derive(Clone, PartialEq)]
enum Piece {
    Text(String),
    Match { text: String, current: bool },
}

#[component]
pub(crate) fn SearchHighlights(state: EditorState) -> impl IntoView {
    view! {
        <div
            class="config-editor__search-highlights"
            aria-hidden="true"
            style:transform=move || format!("translate({}px, {}px)", -state.view().scroll_x.get(), -state.view().scroll_y.get())
        >
            {move || mirror_rows(state).into_iter().map(mirror_row).collect_view()}
        </div>
    }
}

fn mirror_row(found: MirrorRow) -> impl IntoView {
    view! {
        <div class="config-editor__search-line" style=cssom(row_box(0, found.row, None))>
            {found.pieces.into_iter().map(piece).collect_view()}
        </div>
    }
}

fn piece(piece: Piece) -> impl IntoView {
    match piece {
        Piece::Text(text) => text.into_any(),
        // A match that is only a line break (or empty) has no glyphs to
        // cover: the bare class gives it a half-column mark.
        Piece::Match { text, current } => {
            let bare = text.is_empty();
            view! {
            <span
                class="config-editor__search-match"
                class:config-editor__search-match-current=current
                class:config-editor__search-match-bare=bare
            >
                {text}
            </span>
            }
            .into_any()
        }
    }
}

/// The mirrored lines for the matches on the rendered rows.
fn mirror_rows(state: EditorState) -> Vec<MirrorRow> {
    let search = state.search();
    let rows = state.view().visible_rows.get();
    let current = search.current.get();
    let projection = state.view().projection.get();
    search.results.with(|results| {
        if results.matches.is_empty() || rows.is_empty() {
            return Vec::new();
        }
        // The document lines on the rendered rows, folded ones skipped.
        let rows = projection.line_of_row(rows.start)..projection.line_of_row(rows.end - 1) + 1;
        state.doc().text.with(|text| {
            // A match starting above the rows may run into them.
            let first = results
                .matches
                .partition_point(|found| found.line < rows.start)
                .saturating_sub(1);
            let mut segments = Vec::new();
            for (index, found) in results.matches.iter().enumerate().skip(first) {
                if found.line >= rows.end || segments.len() >= MAX_MATCHES {
                    break;
                }
                push_segments(text, found, Some(index) == current, &rows, &mut segments);
            }
            segments
                .chunk_by(|a, b| a.line == b.line)
                .filter(|line| !projection.is_hidden(line[0].line))
                .map(|line| mirror_line(text, line, projection.row_of_line(line[0].line)))
                .collect()
        })
    })
}

/// The pieces of one line's text, cut at `segments` (its matches, in order).
/// The whole line, not the part up to the last match: where the text of a
/// right-to-left run lands depends on all of it.
fn mirror_line(text: &str, segments: &[Segment], row: usize) -> MirrorRow {
    let line_start = segments[0].line_start;
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |newline| line_start + newline);
    let mut pieces = Vec::new();
    let mut cursor = line_start;
    for segment in segments {
        let start = segment.start.max(cursor);
        let end = segment.end.max(start);
        if cursor < start {
            pieces.push(Piece::Text(text[cursor..start].to_string()));
        }
        pieces.push(Piece::Match {
            text: text[start..end].to_string(),
            current: segment.current,
        });
        cursor = end;
    }
    if cursor < line_end {
        pieces.push(Piece::Text(text[cursor..line_end].to_string()));
    }
    MirrorRow { row, pieces }
}

/// One segment per line `found` covers within `rows`, each on its document
/// line.
fn push_segments(
    text: &str,
    found: &SearchMatch,
    current: bool,
    rows: &Range<usize>,
    segments: &mut Vec<Segment>,
) {
    let (mut line, mut line_start, mut start) = (found.line, found.line_start, found.start);
    loop {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |newline| line_start + newline);
        let end = found.end.min(line_end);
        if rows.contains(&line) && start <= end {
            segments.push(Segment {
                line,
                line_start,
                start,
                end,
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
