//! The pieces of one problem's underline: one per line it covers.

use super::super::super::diagnostics::Diagnostic;

/// Most lines of one problem underlined (a host's range over a whole file
/// would otherwise draw thousands of underlines).
pub(super) const MAX_LINES: usize = 200;

/// The underline of a problem on one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Segment {
    /// One-based line.
    pub(super) line: usize,
    /// One-based character column where it starts.
    pub(super) column: usize,
    /// One-based character column just past it.
    pub(super) end_column: usize,
    /// The first one drawn: it carries the message and the accessible name.
    pub(super) first: bool,
}

/// The underlines of `diagnostic` over `lines` (the text's lines): on its
/// line from its column, then for a problem over several lines every line
/// after it whole, and the last up to its end column. Lines `hidden` says
/// are folded (by zero-based index) and empty lines get none.
pub(super) fn segments(
    diagnostic: &Diagnostic,
    lines: &[&str],
    hidden: impl Fn(usize) -> bool,
) -> Vec<Segment> {
    let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) else {
        return Vec::new();
    };
    let end_column = diagnostic.end_column.unwrap_or(column + 1);
    let Some(end_line) = diagnostic.end_line.filter(|end| *end > line) else {
        if hidden(line - 1) {
            return Vec::new();
        }
        return vec![Segment {
            line,
            column,
            end_column,
            first: true,
        }];
    };
    let last = end_line
        .min(line + MAX_LINES - 1)
        .min(lines.len().max(line));
    let mut drawn: Vec<Segment> = Vec::new();
    for at in line..=last {
        if hidden(at - 1) {
            continue;
        }
        let width = lines.get(at - 1).map_or(0, |text| text.chars().count()) + 1;
        let from = if at == line { column } else { 1 };
        let to = if at == end_line {
            end_column.min(width)
        } else {
            width
        };
        // The start always shows (at a line's end it marks one character).
        if at != line && to <= from {
            continue;
        }
        drawn.push(Segment {
            line: at,
            column: from,
            end_column: to.max(from + 1),
            first: drawn.is_empty(),
        });
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::{MAX_LINES, Segment, segments};
    use crate::Diagnostic;

    const TEXT: &str = "listen: 7835\nweb:\n\n  http: 80\n  hosts: []\n";

    fn over(line: usize, column: usize, end_line: Option<usize>, end_column: usize) -> Diagnostic {
        Diagnostic {
            line: Some(line),
            column: Some(column),
            end_line,
            end_column: Some(end_column),
            ..Diagnostic::default()
        }
    }

    /// `(line, column, end_column)` of each underline, the first marked.
    fn drawn(diagnostic: &Diagnostic, folded: &[usize]) -> Vec<(usize, usize, usize, bool)> {
        let lines: Vec<&str> = TEXT.split('\n').collect();
        segments(diagnostic, &lines, |index| folded.contains(&(index + 1)))
            .into_iter()
            .map(
                |Segment {
                     line,
                     column,
                     end_column,
                     first,
                 }| (line, column, end_column, first),
            )
            .collect()
    }

    #[test]
    fn a_problem_over_several_lines_is_underlined_on_each() {
        // Three lines: from its column, whole lines between (the empty one
        // skipped), up to its end column.
        assert_eq!(
            drawn(&over(1, 9, Some(4), 7), &[]),
            [(1, 9, 13, true), (2, 1, 5, false), (4, 1, 7, false)]
        );
        // One line, as always.
        assert_eq!(drawn(&over(1, 9, None, 13), &[]), [(1, 9, 13, true)]);
        // An end past its last line's end, or past the text: kept inside.
        assert_eq!(
            drawn(&over(4, 3, Some(5), 99), &[]),
            [(4, 3, 11, true), (5, 1, 12, false)]
        );
        assert_eq!(drawn(&over(5, 3, Some(40), 2), &[]), [(5, 3, 12, true)]);
    }

    #[test]
    fn folded_lines_get_no_underline_and_the_first_shown_carries_the_message() {
        assert_eq!(
            drawn(&over(1, 9, Some(5), 4), &[2, 3, 4]),
            [(1, 9, 13, true), (5, 1, 4, false)]
        );
        assert_eq!(
            drawn(&over(1, 9, Some(5), 4), &[1, 2]),
            [(4, 1, 11, true), (5, 1, 4, false)]
        );
        assert_eq!(drawn(&over(2, 1, None, 4), &[2]), []);
    }

    #[test]
    fn at_most_two_hundred_lines_of_one_problem_are_underlined() {
        let text = "x\n".repeat(1000);
        let lines: Vec<&str> = text.split('\n').collect();
        let found = segments(&over(1, 1, Some(900), 2), &lines, |_| false);
        assert_eq!(found.len(), MAX_LINES);
    }
}
