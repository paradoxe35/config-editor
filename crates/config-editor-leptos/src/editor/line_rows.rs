use super::changes::{LineDiff, RowPeeks, line_kind, row_peeks};
use super::diagnostics::{Diagnostic, Severity};
use super::diff::DiffKind;
use super::fold_ranges::FoldRange;
use super::languages::Language;
use super::protect::ProtectState;
use crate::reactive::{Fanout, notify};
use leptos::prelude::*;
use std::ops::Range;

/// One line of the document plus its error flag.
#[derive(Clone, PartialEq)]
pub(super) struct RenderedLine {
    pub(super) text: String,
    pub(super) has_error: bool,
    /// Protected text on the line: muted values and the gutter lock (boxed:
    /// most lines have none, and every line carries the field).
    pub(super) marks: Option<Box<LineMarks>>,
}

/// Each line of `text` with its error flag (from `diagnostics`) and its
/// protected-text marks.
pub(super) fn rendered_lines(
    text: RwSignal<String>,
    diagnostics: Memo<Vec<Diagnostic>>,
    protect: ProtectState,
) -> Memo<Vec<RenderedLine>> {
    Memo::new(move |_| {
        let error_lines: std::collections::HashSet<usize> = diagnostics.with(|errors| {
            errors
                .iter()
                .filter(|error| error.severity == Severity::Error)
                .filter_map(|error| error.line)
                .collect()
        });
        text.with(|text| {
            let marks = protect.line_marks(text);
            text.split('\n')
                .enumerate()
                .map(|(index, line)| RenderedLine {
                    text: line.to_string(),
                    has_error: error_lines.contains(&(index + 1)),
                    marks: marks
                        .get(index)
                        .filter(|marks| **marks != LineMarks::default())
                        .map(|marks| Box::new(marks.clone())),
                })
                .collect::<Vec<_>>()
        })
    })
}

/// What one line shows of protected text (see `protect`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LineMarks {
    /// Byte ranges of the line drawn muted.
    pub(super) muted: Vec<Range<usize>>,
    /// The gutter lock's label, with the reason.
    pub(super) lock: Option<String>,
    /// Byte ranges of the line's secret placeholders.
    pub(super) secret: Vec<Range<usize>>,
    /// The gutter key's label, on a line with a placeholder.
    pub(super) key: Option<String>,
}

impl LineMarks {
    /// The byte ranges drawn specially, each with its class.
    pub(super) fn classes(&self) -> Vec<(Range<usize>, &'static str)> {
        // A placeholder in a read-only value is drawn as a placeholder.
        self.secret
            .iter()
            .map(|range| (range.clone(), "config-editor__secret"))
            .chain(
                self.muted
                    .iter()
                    .map(|range| (range.clone(), "config-editor__locked")),
            )
            .collect()
    }
}

/// A row's protected-text marks; a closed fold on it that hides a
/// read-only line (or a secret placeholder) shows its lock (or key) too.
fn folded_marks(row: Option<&RenderedLine>, hidden: &[RenderedLine]) -> Option<Box<LineMarks>> {
    let mut marks = row.and_then(|row| row.marks.clone());
    let inside = |pick: fn(&LineMarks) -> &Option<String>| {
        hidden
            .iter()
            .find_map(|line| line.marks.as_deref().and_then(|marks| pick(marks).clone()))
    };
    let lock = inside(|marks| &marks.lock);
    let key = inside(|marks| &marks.key);
    if lock.is_some() || key.is_some() {
        let entry = marks.get_or_insert_with(Default::default);
        if entry.lock.is_none() {
            entry.lock = lock;
        }
        if entry.key.is_none() {
            entry.key = key;
        }
    }
    marks
}

/// The closed folds (sorted), for a folded row to show the errors it hides.
#[derive(Clone, Copy, Default)]
pub(super) struct RowFolds<'a> {
    pub(super) closed: &'a [FoldRange],
}

impl RowFolds<'_> {
    /// The last line the fold closed on `index` hides.
    fn closed_end(&self, index: usize) -> Option<usize> {
        self.closed
            .binary_search_by_key(&index, |fold| fold.start)
            .ok()
            .map(|at| self.closed[at].end)
    }
}

/// Everything one gutter/highlight row displays, comparable so unchanged rows
/// are not notified.
#[derive(Clone, PartialEq)]
pub(super) struct RowUiState {
    pub(super) text: String,
    pub(super) has_error: bool,
    /// How the line differs from the baseline.
    pub(super) change: DiffKind,
    pub(super) is_active: bool,
    /// The previews its change markers open (see `changes`).
    pub(super) peeks: RowPeeks,
    pub(super) marks: Option<Box<LineMarks>>,
    /// A closed fold on this row hides a line with an error.
    pub(super) hidden_error: bool,
    /// Set by [`LineRows`].
    pub(super) highlight: RowHighlight,
}

pub(super) fn row_state(
    index: usize,
    rows: &[RenderedLine],
    line_count: usize,
    line_diff: &LineDiff,
    active_line: usize,
    folds: RowFolds<'_>,
) -> RowUiState {
    let line = index + 1;
    let row = rows.get(index);
    let text = row.map(|row| row.text.clone()).unwrap_or_default();
    let change = line_kind(line_diff, index);
    let hidden = folds
        .closed_end(index)
        .and_then(|end| rows.get(index + 1..=end.min(rows.len().saturating_sub(1))))
        .unwrap_or_default();
    RowUiState {
        marks: folded_marks(row, hidden),
        hidden_error: hidden.iter().any(|row| row.has_error),
        highlight: RowHighlight::Plain,
        has_error: row.is_some_and(|row| row.has_error),
        change,
        is_active: line == active_line,
        peeks: row_peeks(line_diff, index, line_count, &text, change),
        text,
    }
}

/// How a row's text is highlighted. [`LineRows`] sets it; [`row_state`]
/// leaves it plain.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum RowHighlight {
    /// Highlighting is off.
    #[default]
    Plain,
    /// A line inside a multi-line string's body: all string.
    StringBody,
    /// Tokens of the language.
    Language(Language),
}

/// What highlighting needs to know of the document.
#[derive(Clone, Copy)]
pub(super) struct Highlighting<'a> {
    /// The language, when highlighting is on.
    pub(super) language: Option<Language>,
    /// The lines inside multi-line string bodies.
    pub(super) string_bodies: &'a [Range<usize>],
}

impl Highlighting<'_> {
    fn of(&self, index: usize) -> RowHighlight {
        match self.language {
            None => RowHighlight::Plain,
            Some(_)
                if self
                    .string_bodies
                    .iter()
                    .any(|lines| lines.contains(&index)) =>
            {
                RowHighlight::StringBody
            }
            Some(language) => RowHighlight::Language(language),
        }
    }
}

/// Everything a row's state is built from.
#[derive(Clone, Copy)]
pub(super) struct RowInputs<'a> {
    pub(super) rows: &'a [RenderedLine],
    pub(super) line_diff: &'a LineDiff,
    pub(super) active_line: usize,
    pub(super) folds: RowFolds<'a>,
    pub(super) highlighting: Highlighting<'a>,
}

impl RowInputs<'_> {
    fn state(&self, index: usize) -> RowUiState {
        RowUiState {
            highlight: self.highlighting.of(index),
            ..row_state(
                index,
                self.rows,
                self.rows.len(),
                self.line_diff,
                self.active_line,
                self.folds,
            )
        }
    }
}

/// Per-row display state for the keyed gutter and highlight rows.
///
/// Each row index has a slot with its state, and [`LineRows::sync`] writes
/// only the slots whose state changed. Rows must not subscribe to the
/// document-wide memos: with thousands of rows, every keystroke would re-run
/// every row's closures and churn subscriptions, which is quadratic.
///
/// A rendered row reads its slot through a signal of its own
/// ([`LineRows::state_for`]), which goes with the row: rows come and go as
/// the view scrolls, and one subscribing to a signal that outlives it would
/// stay in that signal's subscribers for good (see `reactive`).
///
/// Slots are positional (row `i` shows line `i + 1`) and the list only
/// grows; unused tail entries are kept for reuse.
#[derive(Clone, Copy)]
pub(super) struct LineRows {
    slots: StoredValue<Vec<Fanout<RowUiState>>>,
}

impl LineRows {
    pub(super) fn new() -> Self {
        Self {
            slots: StoredValue::new(Vec::new()),
        }
    }

    /// A signal with the state of row `index`, owned by the calling row.
    pub(super) fn state_for(&self, index: usize, inputs: &RowInputs<'_>) -> RwSignal<RowUiState> {
        self.slots
            .try_update_value(|slots| {
                extend(slots, index + 1, inputs);
                slots[index].reader()
            })
            .expect("rows are built while the editor that keeps their slots is mounted")
    }

    /// Writes the slots of the `visible` lines whose state changed. Rows
    /// outside them are not rendered; they are brought up to date when they
    /// scroll in.
    ///
    /// Only changes are written: each write re-renders the row.
    pub(super) fn sync(&self, inputs: &RowInputs<'_>, visible: &[usize]) {
        let visible: Vec<usize> = visible
            .iter()
            .copied()
            .filter(|index| *index < inputs.rows.len())
            .collect();
        let needed = visible.iter().max().map_or(0, |last| last + 1);
        // The rows are written once the slots are let go: a row's
        // subscribers may run at once.
        let changed: Vec<(Vec<RwSignal<RowUiState>>, RowUiState)> = self
            .slots
            .try_update_value(|slots| {
                extend(slots, needed, inputs);
                visible
                    .iter()
                    .filter_map(|&index| {
                        let next = inputs.state(index);
                        (slots[index].value() != &next)
                            .then(|| (slots[index].set(next.clone()), next))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (readers, next) in changed {
            notify(readers, &next);
        }
    }
}

/// Adds slots until there are `len`.
fn extend(slots: &mut Vec<Fanout<RowUiState>>, len: usize, inputs: &RowInputs<'_>) {
    while slots.len() < len {
        slots.push(Fanout::new(inputs.state(slots.len())));
    }
}
