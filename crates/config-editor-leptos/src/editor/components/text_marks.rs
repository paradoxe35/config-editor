//! Boxes behind ranges of the text: the find widget's matches (the current
//! one distinct) and a jump's flash (see `jump`). Only matches on rendered
//! rows are drawn, so a document with thousands of matches costs no more
//! than one with a screenful.
//!
//! Each row with a mark is a copy of its line in the textarea's font, its
//! text invisible and each mark a span with the fill, so the browser lays
//! the boxes out as it lays out the text: under the user's letter and word
//! spacing, in right-to-left and mixed-direction runs, over tabs and wide
//! characters, with no arithmetic on columns that has to agree with it.

use std::ops::Range;

use leptos::prelude::*;

use super::super::paths::line_of_byte;
use super::super::positioning::row_box;
use super::super::state::EditorState;
use crate::cssom::cssom;
use crate::editor::text::{TextRange, TextSize, slice};

/// Most lines a jump's flash covers (the host may reveal a whole file).
const MAX_FLASH_LINES: usize = 200;

/// What a marked range is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(
    not(feature = "search"),
    expect(dead_code, reason = "only find draws matches")
)]
pub(in crate::editor) enum Mark {
    /// A find match.
    Match,
    /// The current find match.
    CurrentMatch,
    /// A jump's target, fading out.
    Flash,
}

impl Mark {
    fn class(self) -> &'static str {
        match self {
            Self::Match => "config-editor__search-match",
            Self::CurrentMatch => "config-editor__search-match config-editor__search-match-current",
            Self::Flash => "config-editor__jump-flash",
        }
    }
}

/// The part of a marked range on one document line.
#[derive(Clone, Copy)]
pub(in crate::editor) struct Segment {
    line: usize,
    line_start: TextSize,
    range: TextRange,
    mark: Mark,
}

/// A line with marks: its zero-based textarea row and the pieces of its
/// text, marked or not, in order.
#[derive(Clone, PartialEq)]
struct MirrorRow {
    row: usize,
    pieces: Vec<Piece>,
}

#[derive(Clone, PartialEq)]
enum Piece {
    Text(String),
    Marked { text: String, mark: Mark },
}

#[component]
pub(crate) fn TextMarks(state: EditorState) -> impl IntoView {
    view! {
        <div
            class="config-editor__text-marks"
            aria-hidden="true"
            style:transform=move || format!("translate({}px, {}px)", -state.view().scroll_x.get(), -state.view().scroll_y.get())
        >
            {move || search_rows(state).into_iter().map(mirror_row).collect_view()}
            // Apart from the matches: drawn again only when the flash, the
            // text or the folds change, not on every scroll (which would
            // start the fade over).
            {move || flash_view(state)}
        </div>
    }
}

fn mirror_row(found: MirrorRow) -> impl IntoView {
    view! {
        <div class="config-editor__text-mark-line" style=cssom(row_box(0, found.row, None))>
            {found.pieces.into_iter().map(piece).collect_view()}
        </div>
    }
}

fn piece(piece: Piece) -> impl IntoView {
    match piece {
        Piece::Text(text) => text.into_any(),
        // A mark that is only a line break (or empty) has no glyphs to
        // cover: the bare class gives it a half-column mark.
        Piece::Marked { text, mark } => {
            let bare = text.is_empty();
            view! {
            <span class=mark.class() class:config-editor__mark-bare=bare>
                {text}
            </span>
            }
            .into_any()
        }
    }
}

/// The mirrored lines for the find matches on the rendered rows.
fn search_rows(state: EditorState) -> Vec<MirrorRow> {
    if !state.config().settings.search.get() {
        return Vec::new();
    }
    let rows = state.view().visible_rows.get();
    let projection = state.view().projection.get();
    if rows.is_empty() {
        return Vec::new();
    }
    // The document lines on the rendered rows, folded ones skipped.
    let rows = projection.line_of_row(rows.start)..projection.line_of_row(rows.end - 1) + 1;
    state.doc().text.with(|text| {
        let segments = state.search_marks(text, &rows);
        segments
            .chunk_by(|a, b| a.line == b.line)
            .filter(|line| !projection.is_hidden(line[0].line))
            .map(|line| mirror_line(text, line, projection.row_of_line(line[0].line)))
            .collect()
    })
}

/// The jump's flash: a soft bar across each row of its lines (seen beside
/// the selection, which covers the range itself), and for a range an edge
/// around its text.
fn flash_view(state: EditorState) -> Option<AnyView> {
    let flash = state.shown_flash()?;
    let projection = state.view().projection.get();
    state.doc().text.with(|text| {
        let first = line_of_byte(text, flash.range.start());
        let last = line_of_byte(text, flash.range.end()).min(first + MAX_FLASH_LINES - 1);
        // Across the visible width wherever the text is scrolled to.
        let left = state.view().scroll_x.get_untracked();
        let bars = (first..=last)
            .filter(|line| !projection.is_hidden(*line))
            .map(|line| {
                let style =
                    row_box(0, projection.row_of_line(line), None).set("left", format!("{left}px"));
                view! { <div class="config-editor__jump-flash-line" style=cssom(style)></div> }
            })
            .collect_view();
        let mut segments = Vec::new();
        if !flash.is_place() {
            let line_start = slice::line_start_of(text, flash.range.start());
            let marked = Segment::new(first, line_start, flash.range, Mark::Flash);
            push_segments(text, marked, &(first..last + 1), &mut segments);
        }
        let ranges = segments
            .iter()
            .filter(|segment| !projection.is_hidden(segment.line))
            .map(|segment| {
                mirror_line(
                    text,
                    std::slice::from_ref(segment),
                    projection.row_of_line(segment.line),
                )
            })
            .map(mirror_row)
            .collect_view();
        Some(view! { {bars} {ranges} }.into_any())
    })
}

/// The pieces of one line's text, cut at `segments` (its marks, in order).
/// The whole line, not the part up to the last mark: where the text of a
/// right-to-left run lands depends on all of it.
fn mirror_line(text: &str, segments: &[Segment], row: usize) -> MirrorRow {
    let line_start = segments[0].line_start;
    let line_end = slice::line_end_of(text, line_start);
    let cut = |start, end| {
        slice::slice(text, TextRange::new(start, end))
            .unwrap_or("")
            .to_string()
    };
    let mut pieces = Vec::new();
    let mut cursor = line_start;
    for segment in segments {
        let start = segment.range.start().max(cursor);
        let end = segment.range.end().max(start);
        if cursor < start {
            pieces.push(Piece::Text(cut(cursor, start)));
        }
        pieces.push(Piece::Marked {
            text: cut(start, end),
            mark: segment.mark,
        });
        cursor = end;
    }
    if cursor < line_end {
        pieces.push(Piece::Text(cut(cursor, line_end)));
    }
    MirrorRow { row, pieces }
}

impl Segment {
    /// The range `range` starting on (zero-based) `line`, which starts at
    /// byte `line_start`, marked `mark`.
    pub(in crate::editor) fn new(
        line: usize,
        line_start: TextSize,
        range: TextRange,
        mark: Mark,
    ) -> Self {
        Self {
            line,
            line_start,
            range,
            mark,
        }
    }
}

/// One segment per line `marked` covers within `lines`, each on its
/// document line.
pub(in crate::editor) fn push_segments(
    text: &str,
    marked: Segment,
    lines: &Range<usize>,
    segments: &mut Vec<Segment>,
) {
    let (mut line, mut line_start, mut start) =
        (marked.line, marked.line_start, marked.range.start());
    let newline = TextSize::from(1);
    loop {
        let line_end = slice::line_end_of(text, line_start);
        let end = marked.range.end().min(line_end);
        if lines.contains(&line) && start <= end {
            segments.push(Segment {
                line,
                line_start,
                range: TextRange::new(start, end),
                mark: marked.mark,
            });
        }
        if marked.range.end() <= line_end + newline || line + 1 >= lines.end {
            return;
        }
        line += 1;
        line_start = line_end + newline;
        start = line_start;
    }
}
