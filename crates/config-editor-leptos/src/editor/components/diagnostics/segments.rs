//! The pieces of one problem's underline: one per line it covers.

use super::super::super::diagnostics::Diagnostic;

/// Most lines of one problem underlined (a host's range over a whole file
/// would otherwise draw thousands of underlines).
pub(super) const MAX_LINES: usize = 200;

/// Most underlines drawn in all, every problem's lines counted: past them
/// problems are still counted, listed and reached with F8 (200 ranges of
/// 200 lines each drew 40,000 underlines and hung the page).
pub(crate) const MAX_DRAWN: usize = 200;

/// The underlines to draw, and whether the cap left some out.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Plan {
    /// Each underline with its problem's index.
    pub(super) drawn: Vec<(usize, Segment)>,
    /// [`MAX_DRAWN`] left lines of some problems without an underline (the
    /// footer says so).
    pub(super) cut: bool,
}

/// The underlines to draw for `problems`, each over at most [`MAX_LINES`]
/// lines, until [`MAX_DRAWN`] in all, the most serious problems first
/// (errors, then warnings, info and hints; within each, in the order they
/// are listed: the editor's own, then the host's). Within a severity,
/// first every problem's start, then the rest of their lines. So neither a
/// range over the whole file nor a crowd of hints hides an error's
/// underline, nor one wide error the next. Picked by seriousness, not by
/// nearness to the view, so scrolling never redraws them.
pub(super) fn plan(
    problems: &[Diagnostic],
    lines: &[&str],
    hidden: impl Fn(usize) -> bool,
) -> Plan {
    let mut order: Vec<usize> = (0..problems.len()).collect();
    // A stable sort: the list order within each severity.
    order.sort_by_key(|index| problems[*index].severity as u8);
    let mut planned = Plan::default();
    for kind in order.chunk_by(|a, b| problems[*a].severity == problems[*b].severity) {
        planned.add(
            kind.iter().map(|index| (*index, &problems[*index])),
            lines,
            &hidden,
        );
        if planned.cut {
            break;
        }
    }
    planned
}

impl Plan {
    /// Adds the underlines of `problems` (of one severity) while there is
    /// room: every start, then the rest of their lines.
    fn add<'a>(
        &mut self,
        problems: impl Iterator<Item = (usize, &'a Diagnostic)>,
        lines: &[&str],
        hidden: &impl Fn(usize) -> bool,
    ) {
        // Each problem's lines after its start, for the second round.
        let mut rest: Vec<(usize, std::vec::IntoIter<Segment>)> = Vec::new();
        for (index, problem) in problems {
            let mut found = segments(problem, lines, hidden).into_iter();
            let Some(start) = found.next() else {
                continue;
            };
            if self.drawn.len() == MAX_DRAWN {
                self.cut = true;
                break;
            }
            self.drawn.push((index, start));
            rest.push((index, found));
        }
        for (index, more) in rest {
            let left = MAX_DRAWN - self.drawn.len();
            self.cut |= more.len() > left;
            self.drawn
                .extend(more.take(left).map(|segment| (index, segment)));
        }
    }
}

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
    use super::{MAX_DRAWN, MAX_LINES, Segment, plan, segments};
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

    /// QA 19: the cap counts every line drawn, not every problem: 200
    /// ranges of 200 lines draw 200 underlines, not 40,000, quickly.
    #[test]
    fn the_cap_counts_every_underlined_line_of_every_problem() {
        let text = "key: value\n".repeat(3000);
        let lines: Vec<&str> = text.split('\n').collect();
        let problems: Vec<Diagnostic> = (0..200)
            .map(|index| over(1 + index * 7, 1, Some(1 + index * 7 + 200), 2))
            .collect();
        let (took, planned) =
            crate::editor::test_timing::best_of(3, || plan(&problems, &lines, |_| false));
        assert_eq!(planned.drawn.len(), MAX_DRAWN);
        assert!(planned.cut);
        // Every problem's start first (QA 20: the first one took the whole
        // budget, hiding the others).
        let mut starts: Vec<usize> = planned.drawn.iter().map(|(index, _)| *index).collect();
        starts.dedup();
        assert_eq!(starts, (0..200).collect::<Vec<_>>());
        assert!(took.as_millis() < 50, "planning took {took:?}");
        // One-line problems: the first 200 of them.
        let short: Vec<Diagnostic> = (0..300).map(|index| over(1 + index, 1, None, 3)).collect();
        let planned = plan(&short, &lines, |_| false);
        assert_eq!(planned.drawn.len(), MAX_DRAWN);
        assert_eq!(planned.drawn.last().map(|(index, _)| *index), Some(199));
        assert!(planned.cut);
        // Mixed: every start, then a long range takes what is left.
        let mut mixed: Vec<Diagnostic> =
            (0..150).map(|index| over(1 + index, 1, None, 3)).collect();
        mixed.push(over(1, 1, Some(500), 2));
        mixed.push(over(2, 1, None, 3));
        let planned = plan(&mixed, &lines, |_| false);
        assert_eq!(planned.drawn.len(), MAX_DRAWN);
        let of = |wanted: usize| {
            planned
                .drawn
                .iter()
                .filter(|(index, _)| *index == wanted)
                .count()
        };
        assert_eq!((of(150), of(151)), (49, 1));
        // Under the cap nothing is cut.
        let planned = plan(&short[..20], &lines, |_| false);
        assert_eq!((planned.drawn.len(), planned.cut), (20, false));
    }

    /// QA 20: the cap went in list order, so a wide info (or 250 hints
    /// listed first) hid a later error's underline. The most serious
    /// problems are drawn first, each kind in list order, across every
    /// mix of severities.
    #[test]
    fn the_cap_draws_the_most_serious_problems_first() {
        use crate::Severity::{self, Error, Hint, Info, Warning};
        let text = "key: value\n".repeat(600);
        let lines: Vec<&str> = text.split('\n').collect();
        let with = |severity: Severity, problem: Diagnostic| Diagnostic {
            severity,
            ..problem
        };
        let mut failures = Vec::new();
        let kinds = [Error, Warning, Info, Hint];
        for crowd in kinds {
            for wanted in kinds
                .iter()
                .copied()
                .filter(|wanted| (*wanted as u8) < (crowd as u8))
            {
                let cases: [(&str, Vec<Diagnostic>); 2] = [
                    (
                        "a range over 400 lines",
                        vec![with(crowd, over(1, 1, Some(400), 2))],
                    ),
                    (
                        "250 one-line problems",
                        (0..250)
                            .map(|index| with(crowd, over(30 + index, 1, None, 3)))
                            .collect(),
                    ),
                ];
                for (name, mut problems) in cases {
                    problems.push(with(wanted, over(10, 1, None, 4)));
                    problems.push(with(wanted, over(500, 1, Some(503), 2)));
                    let planned = plan(&problems, &lines, |_| false);
                    let last = problems.len() - 1;
                    let drawn =
                        |index: usize| planned.drawn.iter().filter(|(at, _)| *at == index).count();
                    if (drawn(last - 1), drawn(last)) != (1, 4)
                        || planned.drawn.len() != MAX_DRAWN
                        || !planned.cut
                    {
                        failures.push(format!(
                            "{name} of {crowd:?} then {wanted:?}s: drawn {} and {} lines of them, {} in all, cut {}",
                            drawn(last - 1),
                            drawn(last),
                            planned.drawn.len(),
                            planned.cut
                        ));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
