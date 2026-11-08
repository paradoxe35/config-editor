//! Underlines for problems. Their messages show in the problem tip (see
//! `tip`), on hover, focus, a tap or F8.

use std::collections::HashMap;

use leptos::prelude::*;

use super::super::a11y::problem_marker;
use super::super::completion::visual_column;
use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use super::super::positioning::row_box;
use super::super::state::EditorState;
use crate::EditorStrings;
use crate::cssom::cssom;
use crate::reactive::Relay;
use segments::{Segment, plan};

mod entries;
mod segments;
mod tip;

pub(crate) use entries::{Entry, entries_view, problems_at};
/// Most underlines drawn in all (see `segments`).
pub(super) use segments::MAX_DRAWN as MAX_UNDERLINES;
pub(crate) use tip::ProblemTip;

#[component]
pub(crate) fn DiagnosticOverlay(state: EditorState) -> impl IntoView {
    // The underlines are redrawn after every check, so they read the
    // problem F8 showed through a relay (see `reactive`).
    let shown = Relay::new(move || state.problems().current.get());
    // Thousands of a host's problems (or lines of one) would slow every
    // key: all are counted and walked with F8, the most serious drawn. A
    // folded line's problem shows on the fold's line in the gutter, not as
    // an underline.
    let planned = Memo::new(move |_| {
        let projection = state.view().projection.get();
        state.doc().text.with(|text| {
            let lines: Vec<&str> = text.split('\n').collect();
            state
                .problems()
                .diagnostics
                .with(|problems| plan(problems, &lines, |line| projection.is_hidden(line)))
        })
    });
    Effect::new(move |_| note_cut(state, planned.with(|planned| planned.cut)));
    view! {
        <div
            class="config-editor__diagnostic-overlay"
            aria-label=state.language_text(|strings| &strings.problems_label)
            style:transform=move || format!("translate({}px, {}px)", -state.view().scroll_x.get(), -state.view().scroll_y.get())
        >
            {move || {
                let text = state.doc().text.get();
                let projection = state.view().projection.get();
                let shown = shown.read();
                // New texts redraw the underlines' names and details.
                state.config().settings.strings.track();
                // Each underline finds its line here, not by counting lines.
                let lines: Vec<&str> = text.split('\n').collect();
                let problems = state.problems().diagnostics.get();
                let covered = covered(&problems);
                planned
                    .get()
                    .drawn
                    .into_iter()
                    .filter_map(|(index, segment)| {
                        let line_text = lines.get(segment.line - 1).copied().unwrap_or("");
                        let drawn = Drawn {
                            diagnostic: &problems[index],
                            covered: covered.contains(&index),
                            segment,
                            row: projection.row_of_line(segment.line - 1),
                        };
                        marker(state, shown, line_text, drawn)
                    })
                    .collect_view()
            }}
        </div>
    }
}

/// Tells the footer whether the underline cap left some problems' lines
/// without an underline.
fn note_cut(state: EditorState, cut: bool) {
    let underlines_cut = state.problems().underlines_cut;
    if underlines_cut.get_untracked() != cut {
        underlines_cut.set(cut);
    }
}

/// The problems drawn over exactly the same text as a more serious one:
/// their underlines would show through (or hide) the more serious one's, so
/// only the most serious problem's colour is drawn there.
fn covered(problems: &[Diagnostic]) -> Vec<usize> {
    let span = |problem: &Diagnostic| {
        (
            problem.line,
            problem.column,
            problem.end_line.or(problem.line),
            problem.end_column,
        )
    };
    // Severities in order: the error first.
    let mut worst: HashMap<_, u8> = HashMap::new();
    for problem in problems.iter().filter(|problem| problem.line.is_some()) {
        let severity = problem.severity as u8;
        worst
            .entry(span(problem))
            .and_modify(|worst| *worst = (*worst).min(severity))
            .or_insert(severity);
    }
    problems
        .iter()
        .enumerate()
        .filter(|(_, problem)| {
            worst
                .get(&span(problem))
                .is_some_and(|worst| *worst < problem.severity as u8)
        })
        .map(|(index, _)| index)
        .collect()
}

/// One underline to draw: a problem's part on one of its lines.
struct Drawn<'a> {
    diagnostic: &'a Diagnostic,
    /// A more serious problem is drawn over the same text.
    covered: bool,
    segment: Segment,
    /// The textarea row it is drawn on.
    row: usize,
}

/// The underline `drawn`. Only the first underline of a problem over
/// several lines is named for screen readers and marked shown after F8.
/// It takes no pointer events: the text under it gets the clicks, and the
/// pointer over it shows the problem's message (see `input::mouse_move`).
fn marker(
    state: EditorState,
    shown: Signal<Option<(usize, usize)>>,
    line_text: &str,
    drawn: Drawn<'_>,
) -> Option<impl IntoView + use<>> {
    let Drawn {
        diagnostic,
        covered,
        segment,
        row,
    } = drawn;
    let line = diagnostic.line?;
    let column = diagnostic.column?;
    let first = segment.first;
    let width = segment.end_column.saturating_sub(segment.column).max(1);
    let marked: String = line_text
        .chars()
        .skip(segment.column.saturating_sub(1))
        .take(width)
        .collect();
    let at_end = marked.is_empty();
    let before: String = line_text
        .chars()
        .take(segment.column.saturating_sub(1))
        .collect();
    // Rendered columns, so tabs (two columns wide) do not shift the underline.
    let left = visual_column(&before);
    let visual_width = visual_column(&format!("{before}{marked}")) - left;
    let severity = diagnostic.severity;
    let from_host = diagnostic.origin == DiagnosticOrigin::Host;
    let detail = state.strings_untracked(|strings| detail(strings, diagnostic));
    let aria_label =
        state.strings_untracked(|strings| problem_marker(strings, state.label(), diagnostic, line));
    Some(view! {
        <button
            type="button"
            class="config-editor__diagnostic-marker"
            class:config-editor__diagnostic-marker-warning=severity == Severity::Warning
            class:config-editor__diagnostic-marker-info=severity == Severity::Info
            class:config-editor__diagnostic-marker-hint=severity == Severity::Hint
            class:config-editor__diagnostic-marker-host=from_host
            class:config-editor__diagnostic-marker-docs=diagnostic.docs_url.is_some()
            class:config-editor__diagnostic-marker-eof=at_end
            class:config-editor__diagnostic-marker-deprecated=diagnostic.deprecated
            class:config-editor__diagnostic-marker-continued=!first
            class:config-editor__diagnostic-marker-covered=covered
            class:config-editor__diagnostic-marker-shown=move || first && shown.get() == Some((line, column))
            tabindex="-1"
            aria-label=aria_label
            aria-hidden=(!first).then_some("true")
            data-message=diagnostic.message.clone()
            data-detail=detail
            style=cssom(row_box(left, row, Some(visual_width.max(1.0))))
            on:focus=move |_| state.hover_problem((line, column))
            on:blur=move |_| state.leave_problem()
        >
            <span>{marked}</span>
        </button>
    })
}

/// The line under a host problem's message: who reported it and its code
/// (the tip adds its docs link).
pub(super) fn detail(strings: &EditorStrings, diagnostic: &Diagnostic) -> Option<String> {
    if diagnostic.origin != DiagnosticOrigin::Host {
        return None;
    }
    let parts: Vec<&str> = [
        diagnostic.source.as_deref(),
        diagnostic.code.as_deref(),
        diagnostic
            .docs_url
            .as_ref()
            .map(|_| strings.open_docs.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::covered;
    use crate::{Diagnostic, Severity};

    fn at(column: usize, end_column: usize, severity: Severity) -> Diagnostic {
        Diagnostic {
            line: Some(1),
            column: Some(column),
            end_column: Some(end_column),
            severity,
            ..Diagnostic::default()
        }
    }

    /// QA 19: an error and a warning over the same text are drawn in the
    /// error's colour (the warning's underline no longer over it).
    #[test]
    fn only_the_most_serious_problem_over_the_same_text_is_drawn_in_colour() {
        let problems = [
            at(1, 5, Severity::Error),
            at(1, 5, Severity::Warning),
            at(1, 5, Severity::Hint),
            at(1, 9, Severity::Info),
            at(1, 5, Severity::Error),
        ];
        assert_eq!(covered(&problems), [1, 2]);
        // Order does not matter.
        let problems = [at(1, 5, Severity::Hint), at(1, 5, Severity::Warning)];
        assert_eq!(covered(&problems), [0]);
    }
}
