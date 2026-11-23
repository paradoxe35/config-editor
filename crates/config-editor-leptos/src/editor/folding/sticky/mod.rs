//! Sticky scroll, as VS Code's: while the text scrolls, the header lines of
//! the blocks holding the top of the view stay drawn at its top, outermost
//! first (at most `sticky_scroll_max_lines`, and a quarter of the text
//! area); the innermost is pushed up as its block ends; a click on one
//! scrolls to its line and puts the caret there.
//!
//! The blocks are the language's fold ranges (`FoldState::blocks`), mapped
//! through the projection, so it works in every language, a custom one
//! too, in Edit-as views and with folds closed, and is part of the
//! `folding` feature. The lines are decoration (`aria-hidden`: the text
//! itself is what a screen reader reads), drawn from the rows' own states,
//! so they show what the code layer shows (colors, muted read-only values,
//! secret placeholders, never a secret). They never take the focus, and a
//! caret moved under them is scrolled back into view below them.
//!
//! - `scopes`: which lines stick at a scroll position, and where.
//! - `view`: the two layers (the line numbers, the text) and their clicks.

mod scopes;
mod view;

#[cfg(test)]
mod tests;

pub(super) use view::{StickyNumbers, StickyText};

use leptos::prelude::*;

use super::super::components::with_row_inputs;
use super::super::line_rows::LineRows;
use super::super::metrics::read_editor_metrics;
use super::super::source_area::SourceArea;
use super::super::state::EditorState;
use crate::reactive::later;
use scopes::{Layout, Scopes, View, without_bare_roots};

/// One editor's sticky lines: its blocks in rows, and their layout at the
/// text's scroll.
#[derive(Clone, Copy)]
pub(super) struct Sticky {
    scopes: Memo<Scopes>,
    layout: Memo<Layout>,
}

impl Sticky {
    /// The sticky lines of `state`, kept up to date in `rows` (whose
    /// states they are drawn from), and the caret kept clear of them.
    pub(super) fn new(state: EditorState, rows: LineRows) -> Self {
        let settings = state.config().settings;
        let blocks = state.fold().blocks;
        let projection = state.view().projection;
        let text = state.doc().text;
        let language = state.sync().edit_language;
        let scopes = Memo::new(move |_| {
            if !settings.sticky_scroll.get() {
                return Scopes::default();
            }
            let brackets = language.get().behavior().config().brackets;
            blocks.with(|blocks| {
                let blocks = text.with(|text| {
                    without_bare_roots(blocks, text, |character| {
                        brackets.iter().any(|(open, _)| *open == character)
                    })
                });
                projection.with(|projection| Scopes::new(&blocks, projection))
            })
        });
        let layout = Memo::new(move |_| {
            let view = state.view();
            view.layout_revision.track();
            let scroll = view.scroll_y.get();
            let (Some(element), max_lines) = (
                view.textarea_ref.get(),
                settings.sticky_scroll_max_lines.get(),
            ) else {
                return Layout::default();
            };
            let at = view_of(&element, scroll, max_lines);
            // Not scrolled past the first row's top, no header is above it: the
            // blocks are not read (nor made again after each edit).
            if at.top <= 0.0 {
                return Layout::default();
            }
            scopes.with(|scopes| scopes.layout(at))
        });
        let sticky = Self { scopes, layout };
        sync_rows(state, rows, sticky);
        keep_caret_clear(state, sticky);
        state.fold().sticky.set_value(Some(sticky));
        sticky
    }

    /// How far down the lines drawn cover the text area, in pixels.
    pub(super) fn covered(self) -> f64 {
        self.layout.with_untracked(|layout| layout.height)
    }

    /// The lines drawn, as `(depth, line)` (tracked).
    fn items(self) -> Vec<(usize, usize)> {
        self.layout.with(|layout| {
            layout
                .lines
                .iter()
                .map(|line| (line.depth, line.line))
                .collect()
        })
    }

    /// Scrolls document line `line` (0-based, a sticky line's) as high as
    /// it goes below the sticky lines that stay above it, with the caret
    /// at its start, as a click on a sticky line does in VS Code.
    fn jump(self, state: EditorState, line: usize) {
        if state.focus_at(line + 1, 1).is_none() {
            return;
        }
        let Some(area) = state.textarea() else {
            return;
        };
        reveal_row(self, &area, area.row_of_line(line));
    }
}

/// Where the view of the text area `element`, scrolled to `scroll` pixels,
/// is, for at most `max_lines` sticky lines (see [`most_lines`]).
fn view_of(element: &web_sys::HtmlTextAreaElement, scroll: f64, max_lines: usize) -> View {
    let metrics = read_editor_metrics(element);
    let rows = f64::from(element.client_height()) / metrics.line_height.max(1.0);
    View {
        top: scroll - metrics.padding_top,
        line_height: metrics.line_height,
        max_lines: most_lines(rows, max_lines),
    }
}

/// How many sticky lines a view `rows` rows tall shows, for at most
/// `max_lines`: never more than a quarter of its rows, as VS Code keeps
/// them, and at least one.
fn most_lines(rows: f64, max_lines: usize) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small, non-negative count of rows"
    )]
    let quarter = (rows / 4.0).floor().max(1.0) as usize;
    max_lines.min(quarter)
}

/// The view of `area` as it is scrolled now.
fn view_now(state: EditorState, area: &SourceArea) -> View {
    let max_lines = state
        .config()
        .settings
        .sticky_scroll_max_lines
        .get_untracked();
    view_of(area.element(), f64::from(area.scroll_top()), max_lines)
}

/// Scrolls textarea row `row` of `area` to the top, below the sticky lines
/// that stay above it.
fn reveal_row(sticky: Sticky, area: &SourceArea, row: usize) {
    let view = view_now(area.state(), area);
    let top = sticky
        .scopes
        .with_untracked(|scopes| scopes.reveal_top(row, view.line_height, view.max_lines));
    let padding_top = read_editor_metrics(area.element()).padding_top;
    let most = f64::from(area.scroll_height() - area.client_height()).max(0.0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a scroll position in pixels, clamped to the scroll height (an `i32`)"
    )]
    area.set_scroll_top((top + padding_top).clamp(0.0, most).round() as i32);
}

/// Keeps the row states of the sticky lines up to date: rows out of view
/// are not otherwise (see `line_rows`).
fn sync_rows(state: EditorState, rows: LineRows, sticky: Sticky) {
    Effect::new(move |_| {
        let lines: Vec<usize> = sticky.items().into_iter().map(|(_, line)| line).collect();
        if !lines.is_empty() {
            with_row_inputs(state, |inputs| rows.sync(inputs, &lines));
        }
    });
}

/// A caret moved onto a row the sticky lines cover (a key, a click, an
/// edit) scrolls up to show it below them, as a scroll margin: once the
/// browser has scrolled it into view at the top.
fn keep_caret_clear(state: EditorState, sticky: Sticky) {
    Effect::new(move |_| {
        state.doc().caret.track();
        later::next_frame(state, move |state| clear_caret(state, sticky));
    });
}

fn clear_caret(state: EditorState, sticky: Sticky) {
    if !state.config().settings.sticky_scroll.get_untracked() {
        return;
    }
    let Some(area) = state.textarea() else {
        return;
    };
    let view = view_now(state, &area);
    if view.top <= 0.0 {
        return;
    }
    let line = state.doc().current_line.get_untracked().saturating_sub(1);
    let row = area.row_of_line(line);
    if sticky
        .scopes
        .with_untracked(|scopes| scopes.covers(view, row))
    {
        reveal_row(sticky, &area, row);
    }
}
