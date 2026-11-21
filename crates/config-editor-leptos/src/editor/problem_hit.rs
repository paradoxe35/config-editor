//! Which problems cover a place in the text: the character under the
//! pointer, or where F8 or a tap went. Every problem whose underline covers
//! it counts, so nested problems (a range inside a wider one) all show.
//! The underlines themselves take no pointer events: a click, double-click,
//! drag or tap on underlined text reaches the text, as in VS Code.

use super::diagnostics::Diagnostic;
use super::metrics::TextWidths;

/// The one-based character columns `from..to` that `problem`'s underline
/// covers on one-based `line`, `length` characters long (at least one
/// wide), as it is drawn (see `components::diagnostics::segments`): from
/// its column on its first line, whole lines between, up to its end column
/// on its last. `None` when it has none there (an empty line inside a
/// range, a problem without a column).
pub(super) fn span_on(problem: &Diagnostic, line: usize, length: usize) -> Option<(usize, usize)> {
    let (start, column) = (problem.line?, problem.column?);
    let end_column = problem.end_column.unwrap_or(column + 1);
    let Some(end_line) = problem.end_line.filter(|end| *end > start) else {
        return (line == start).then_some((column, end_column.max(column + 1)));
    };
    if line < start || line > end_line {
        return None;
    }
    let width = length + 1;
    let from = if line == start { column } else { 1 };
    let to = if line == end_line {
        end_column.min(width)
    } else {
        width
    };
    if line != start && to <= from {
        return None;
    }
    Some((from, to.max(from + 1)))
}

/// The problems covering the character at one-based `line`:`column` of
/// `line_text`, in their order.
pub(super) fn covering<'a>(
    problems: &'a [Diagnostic],
    line: usize,
    column: usize,
    line_text: &str,
) -> impl Iterator<Item = &'a Diagnostic> {
    let length = line_text.chars().count();
    problems.iter().filter(move |problem| {
        span_on(problem, line, length).is_some_and(|(from, to)| from <= column && column < to)
    })
}

/// The one-based column where the innermost of the problems covering
/// `line`:`column` starts on that line (a problem's own start when it
/// starts there), if any covers it.
pub(super) fn innermost_start(
    problems: &[Diagnostic],
    line: usize,
    column: usize,
    line_text: &str,
) -> Option<usize> {
    let length = line_text.chars().count();
    covering(problems, line, column, line_text)
        .filter_map(|problem| span_on(problem, line, length))
        .map(|(from, _)| from)
        .max()
}

/// Where the tip of the problems covering `line`:`column` points: the
/// drawn column (in `widths`) where the innermost of them starts on that
/// line.
pub(super) fn tip_column(
    problems: &[Diagnostic],
    line: usize,
    column: usize,
    line_text: &str,
    widths: TextWidths,
) -> f64 {
    let from = innermost_start(problems, line, column, line_text).unwrap_or(column);
    let before: String = line_text.chars().take(from.saturating_sub(1)).collect();
    widths.columns(&before)
}

#[cfg(test)]
mod tests {
    use super::{covering, span_on, tip_column};
    use crate::editor::metrics::TextWidths;
    use crate::{Diagnostic, Severity};

    fn range(line: usize, column: usize, end_line: Option<usize>, end_column: usize) -> Diagnostic {
        Diagnostic {
            line: Some(line),
            column: Some(column),
            end_line,
            end_column: Some(end_column),
            message: format!("{line}:{column}-{end_line:?}:{end_column}"),
            severity: Severity::Error,
            ..Diagnostic::default()
        }
    }

    /// A problem inside a wider one showed only its own message on
    /// hover. Every problem covering the place counts, multi-line ranges
    /// too; the character just past a problem is not on it.
    #[test]
    fn every_problem_covering_a_place_counts() {
        let text = "key_number_5: value 5";
        let problems = [
            range(6, 1, None, 22),
            range(6, 3, None, 9),
            range(1, 1, Some(30), 1),
            range(6, 15, None, 15),
        ];
        let at = |column| -> Vec<&str> {
            covering(&problems, 6, column, text)
                .map(|problem| problem.message.as_str())
                .collect()
        };
        assert_eq!(at(5), ["6:1-None:22", "6:3-None:9", "1:1-Some(30):1"]);
        assert_eq!(at(9), ["6:1-None:22", "1:1-Some(30):1"]);
        assert_eq!(at(1), ["6:1-None:22", "1:1-Some(30):1"]);
        // A zero-width problem covers the one character it marks.
        assert_eq!(at(15).len(), 3);
        // Past the line's end only the range's line-end mark (none here).
        assert_eq!(at(23), Vec::<&str>::new());
        // The tip points at the innermost start.
        assert_eq!(tip_column(&problems, 6, 5, text, TextWidths::COUNTED), 2.0);
        assert_eq!(tip_column(&problems, 6, 12, text, TextWidths::COUNTED), 0.0);
    }

    #[test]
    fn spans_follow_the_drawn_underlines() {
        let problem = range(2, 5, Some(4), 3);
        assert_eq!(span_on(&problem, 1, 10), None);
        assert_eq!(span_on(&problem, 2, 10), Some((5, 11)));
        assert_eq!(span_on(&problem, 3, 10), Some((1, 11)));
        assert_eq!(span_on(&problem, 3, 0), None);
        assert_eq!(span_on(&problem, 4, 10), Some((1, 3)));
        assert_eq!(span_on(&problem, 5, 10), None);
        // At a line's end: one character wide.
        assert_eq!(span_on(&range(1, 4, None, 4), 1, 3), Some((4, 5)));
    }
}
