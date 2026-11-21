//! Which header lines stick, and where: the scopes of the text as a tree
//! of textarea rows, and the chain of them holding the top of the view.
//!
//! The scopes are the language's fold ranges (every language has them, a
//! custom one too), so nothing here knows a language. They are mapped to
//! textarea rows through the projection, so a closed fold's lines are no
//! rows and a scope folded shut holds nothing. Built once per change of the
//! ranges or the folds; each scroll then asks [`Scopes::layout`], which costs
//! a binary search and a walk up the tree per sticky line, never a scan.

use std::borrow::Cow;

use super::super::super::fold_ranges::FoldRange;
use super::super::super::projection::Projection;
use super::super::super::work_count::{Work, note};

/// One scope, in textarea rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scope {
    /// The row of its header (the line that sticks).
    header: usize,
    /// The last row of its body, after `header`, never past its parent's.
    last: usize,
    /// The document line (0-based) shown on `header`.
    line: usize,
    /// The index of the scope holding it, if any.
    parent: Option<usize>,
}

/// The scopes of a text, sorted by header row and nested: each lies inside
/// its parent (a range crossing its parent's end is cut there).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Scopes {
    scopes: Vec<Scope>,
}

/// Where the view is, in pixels of the text area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct View {
    /// How far the first row is scrolled past the view's top (the text
    /// area's scroll less its top padding; negative inside the padding).
    pub(crate) top: f64,
    /// The height of a row.
    pub(crate) line_height: f64,
    /// The most lines that stick.
    pub(crate) max_lines: usize,
}

/// A header line drawn at the top of the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StickyLine {
    /// The document line (0-based) it shows.
    pub(crate) line: usize,
    /// Its textarea row.
    pub(crate) row: usize,
    /// Its place among the sticky lines, the outermost 0.
    pub(crate) depth: usize,
    /// Its top, in pixels below the view's top: `depth` rows down, less
    /// what its scope's end pushes it up by.
    pub(crate) top: f64,
}

/// The sticky lines at one scroll position.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    /// Outermost first.
    pub(crate) lines: Vec<StickyLine>,
    /// How far down the view they cover, in pixels.
    pub(crate) height: f64,
}

/// `ranges` (sorted by start, as a language gives them) without an
/// outermost one whose first line of `text` is only an opening bracket
/// (`openers`, the language's): the root `{` of a JSON document holds every
/// line and names none, so the sticky lines start at its first key, as in
/// VS Code. A block nested in it stays, however it opens.
pub(crate) fn without_bare_roots<'a>(
    ranges: &'a [FoldRange],
    text: &str,
    openers: impl Fn(char) -> bool,
) -> Cow<'a, [FoldRange]> {
    let mut lines = text.split('\n').enumerate();
    let mut outer_end = None;
    let mut bare = Vec::new();
    for (index, range) in ranges.iter().enumerate() {
        if outer_end.is_some_and(|end| range.start <= end) {
            continue;
        }
        outer_end = Some(range.end);
        // The starts only grow, so one pass over the lines finds them all.
        let header = lines
            .find(|(line, _)| *line == range.start)
            .map_or("", |(_, header)| header.trim());
        let mut characters = header.chars();
        if characters.next().is_some_and(&openers) && characters.next().is_none() {
            bare.push(index);
        }
    }
    if bare.is_empty() {
        return Cow::Borrowed(ranges);
    }
    Cow::Owned(
        ranges
            .iter()
            .enumerate()
            .filter(|(index, _)| bare.binary_search(index).is_err())
            .map(|(_, range)| *range)
            .collect(),
    )
}

impl Scopes {
    /// The scopes of `ranges` (sorted by start, as a language gives them)
    /// in the rows of `projection`.
    pub(crate) fn new(ranges: &[FoldRange], projection: &Projection) -> Self {
        let mut rows: Vec<(usize, usize)> = ranges
            .iter()
            .map(|range| {
                (
                    projection.row_of_line(range.start),
                    projection.row_of_line(range.end),
                )
            })
            .filter(|(header, last)| last > header)
            .collect();
        // The longest of the scopes sharing a header row.
        rows.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        rows.dedup_by_key(|(header, _)| *header);
        let mut scopes: Vec<Scope> = Vec::with_capacity(rows.len());
        let mut open: Vec<usize> = Vec::new();
        for (header, last) in rows {
            while open
                .last()
                .is_some_and(|&parent| scopes[parent].last < header)
            {
                open.pop();
            }
            let parent = open.last().copied();
            let last = parent.map_or(last, |parent| last.min(scopes[parent].last));
            // Opening on its parent's last row, it holds nothing of its own.
            if last <= header {
                continue;
            }
            open.push(scopes.len());
            scopes.push(Scope {
                header,
                last,
                line: projection.line_of_row(header),
                parent,
            });
        }
        Self { scopes }
    }

    /// The innermost scope whose rows (its header included) hold `row`.
    fn innermost_at(&self, row: usize) -> Option<usize> {
        note(Work::StickyStep);
        let after = self.scopes.partition_point(|scope| scope.header <= row);
        let mut at = after.checked_sub(1);
        // The last scope opening at or before `row` lies inside every scope
        // holding `row`: the innermost of those is it or an ancestor.
        while let Some(index) = at {
            note(Work::StickyStep);
            let scope = self.scopes[index];
            if scope.last >= row {
                return Some(index);
            }
            at = scope.parent;
        }
        None
    }

    /// The outermost scope holding `row` that lies inside `within` (any,
    /// with `None`).
    fn outermost_inside(&self, row: usize, within: Option<usize>) -> Option<usize> {
        let mut at = self.innermost_at(row);
        let mut found = None;
        while let Some(index) = at {
            note(Work::StickyStep);
            if Some(index) == within {
                return found;
            }
            found = Some(index);
            at = self.scopes[index].parent;
        }
        within.is_none().then_some(found).flatten()
    }

    /// The sticky lines of `view`, as VS Code's sticky scroll draws them:
    /// at each depth, the outermost scope inside the one above that holds
    /// the row at that depth's top, while its header has scrolled above
    /// that top; the walk stops at the first header still in view. A line
    /// whose scope ends within its place is pushed up by as much.
    pub(crate) fn layout(&self, view: View) -> Layout {
        let mut layout = Layout::default();
        let height = view.line_height;
        if self.scopes.is_empty() || height <= 0.0 || !view.top.is_finite() {
            return layout;
        }
        let mut within = None;
        for depth in 0..view.max_lines {
            // The pixel row at this depth's top, and the row it falls in.
            let at = view.top + depth as f64 * height;
            if at < 0.0 {
                break;
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a row count from a finite, non-negative scroll"
            )]
            let row = (at / height).floor() as usize;
            let Some(index) = self.outermost_inside(row, within) else {
                break;
            };
            let scope = self.scopes[index];
            if scope.header as f64 * height >= at {
                break;
            }
            let end = (scope.last + 1) as f64 * height - view.top;
            let top = (depth as f64 * height).min(end - height);
            layout.lines.push(StickyLine {
                line: scope.line,
                row: scope.header,
                depth,
                top,
            });
            layout.height = layout.height.max(top + height);
            within = Some(index);
        }
        layout
    }

    /// The `top` (as [`View::top`]) that shows `row` as high as it can go
    /// with no sticky line over it: the row right under the headers that
    /// stay above it there.
    pub(crate) fn reveal_top(&self, row: usize, line_height: f64, max_lines: usize) -> f64 {
        for under in 0..max_lines {
            let top = (row as f64 - under as f64) * line_height;
            let covered = self
                .layout(View {
                    top,
                    line_height,
                    max_lines,
                })
                .height;
            if covered <= under as f64 * line_height + 0.5 {
                return top;
            }
        }
        (row as f64 - max_lines as f64) * line_height
    }

    /// Whether the sticky lines of `view` cover any of `row`.
    pub(crate) fn covers(&self, view: View, row: usize) -> bool {
        let row_top = row as f64 * view.line_height - view.top;
        let covered = self.layout(view).height;
        row_top < covered - 0.5 && row_top + view.line_height > 0.5
    }
}

#[cfg(test)]
impl Scopes {
    /// The sticky lines with the view's top on `row` exactly, as document
    /// lines (0-based).
    pub(crate) fn lines_at(&self, row: usize, max_lines: usize) -> Vec<usize> {
        self.layout(View {
            top: row as f64 * 10.0,
            line_height: 10.0,
            max_lines,
        })
        .lines
        .iter()
        .map(|line| line.line)
        .collect()
    }
}
