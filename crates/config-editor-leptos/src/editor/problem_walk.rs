//! The order F8 and Shift+F8 walk the problems in: by position, several at
//! one position one after another, then those with no position (a whole
//! document's, a field not found), which the footer shows.

use super::commands::cycle_offset;
use super::diagnostics::{Diagnostic, MORE_PROBLEMS_CODE};

/// One stop of the walk: the problem's index, and its UTF-16 offset in the
/// text (`None` for one with no position).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stop {
    pub(super) offset: Option<usize>,
    pub(super) index: usize,
}

/// Every problem of `problems` (but the note that the list was cut) in walk
/// order: placed ones by offset in `text` (those at one offset in the order
/// they were found), then the unplaced ones. One pass over the text finds
/// every line's offset, so thousands of problems cost about as much as one.
pub(super) fn stops(text: &str, problems: &[Diagnostic]) -> Vec<Stop> {
    let lines = LineStarts::new(text);
    let mut stops: Vec<Stop> = problems
        .iter()
        .enumerate()
        .filter(|(_, problem)| problem.code.as_deref() != Some(MORE_PROBLEMS_CODE))
        .map(|(index, problem)| Stop {
            offset: problem
                .line
                .zip(problem.column)
                .map(|(line, column)| lines.offset(line, column)),
            index,
        })
        .collect();
    stops.sort_by_key(|stop| (stop.offset.is_none(), stop.offset, stop.index));
    stops
}

/// Each line of a text with its UTF-16 offset.
struct LineStarts<'a> {
    lines: Vec<(usize, &'a str)>,
    /// The text's length in UTF-16.
    total: usize,
}

impl<'a> LineStarts<'a> {
    fn new(text: &'a str) -> Self {
        let mut offset = 0;
        let lines = text
            .split('\n')
            .map(|line| {
                let start = offset;
                offset += line.encode_utf16().count() + 1;
                (start, line)
            })
            .collect();
        Self {
            lines,
            total: offset.saturating_sub(1),
        }
    }

    /// The UTF-16 offset of one-based (`line`, `column`), clamped to the
    /// line and the text (as `commands::offset_at`).
    fn offset(&self, line: usize, column: usize) -> usize {
        let Some((start, text)) = self.lines.get(line.saturating_sub(1)) else {
            return self.total;
        };
        let within: usize = text
            .chars()
            .take(column.saturating_sub(1))
            .map(char::len_utf16)
            .sum();
        (start + within).min(self.total)
    }
}

/// The position in `stops` F8 (or Shift+F8, `backward`) goes to from the
/// caret at `here`. `last` is the stop it went to before, when the caret is
/// still where that left it: the walk goes on from there, so several
/// problems at one place are each visited. Otherwise it is the first after
/// the caret (before it, backward), then the unplaced ones, wrapping.
pub(super) fn next_stop(
    stops: &[Stop],
    here: usize,
    last: Option<usize>,
    backward: bool,
) -> Option<usize> {
    if stops.is_empty() {
        return None;
    }
    if let Some(last) = last.filter(|last| *last < stops.len()) {
        return Some(if backward {
            last.checked_sub(1).unwrap_or(stops.len() - 1)
        } else {
            (last + 1) % stops.len()
        });
    }
    let placed: Vec<usize> = stops.iter().filter_map(|stop| stop.offset).collect();
    let unplaced = stops.len() - placed.len();
    if backward {
        let before = placed.iter().rposition(|offset| *offset < here);
        return Some(before.unwrap_or(stops.len() - 1));
    }
    match placed.iter().position(|offset| *offset > here) {
        Some(after) => Some(after),
        None if unplaced > 0 => Some(placed.len()),
        None => cycle_offset(&placed, here, false),
    }
}

#[cfg(test)]
mod tests {
    use super::{Stop, next_stop, stops};
    use crate::Diagnostic;

    fn at(line: Option<usize>, message: &str) -> Diagnostic {
        Diagnostic {
            line,
            column: line.map(|_| 1),
            end_column: line.map(|_| 2),
            message: message.to_string(),
            ..Diagnostic::default()
        }
    }

    const TEXT: &str = "a\nb\nc\n";

    /// The messages F8 visits, pressed `times` from the caret at `here`,
    /// the caret moving to each placed stop.
    fn walk(problems: &[Diagnostic], here: usize, times: usize, backward: bool) -> Vec<String> {
        let stops = stops(TEXT, problems);
        let mut caret = here;
        let mut last: Option<(usize, usize)> = None;
        let mut seen = Vec::new();
        for _ in 0..times {
            let continuing = last
                .filter(|(_, at)| *at == caret)
                .map(|(position, _)| position);
            let position = next_stop(&stops, caret, continuing, backward).unwrap();
            let Stop { offset, index } = stops[position];
            caret = offset.unwrap_or(caret);
            last = Some((position, caret));
            seen.push(problems[index].message.clone());
        }
        seen
    }

    #[test]
    fn several_problems_at_one_place_are_each_visited_then_the_unplaced() {
        let problems = [
            at(Some(2), "b1"),
            at(None, "document"),
            at(Some(1), "a"),
            at(Some(2), "b2"),
            at(Some(3), "c"),
        ];
        assert_eq!(
            walk(&problems, 0, 6, false),
            ["b1", "b2", "c", "document", "a", "b1"]
        );
        assert_eq!(
            walk(&problems, 4, 6, true),
            ["b2", "b1", "a", "document", "c", "b2"]
        );
    }

    #[test]
    fn the_note_that_the_list_was_cut_is_no_stop() {
        let mut note = at(None, "only the first 20");
        note.code = Some(super::MORE_PROBLEMS_CODE.to_string());
        let problems = [at(Some(1), "a"), note, at(None, "document")];
        let stops = stops(TEXT, &problems);
        assert_eq!(stops.len(), 2);
        assert_eq!(walk(&problems, 0, 3, false), ["document", "a", "document"]);
    }

    /// The one-pass offsets are `offset_at`'s, clamping included.
    #[test]
    fn line_starts_match_offset_at_everywhere() {
        for text in ["", "a", "a\n", "é😀x\n\nb\r\nlast", "\n\n", "abc\ndé"] {
            let lines = super::LineStarts::new(text);
            for line in 0..7 {
                for column in 0..8 {
                    assert_eq!(
                        lines.offset(line, column),
                        super::super::commands::offset_at(text, line, column),
                        "{text:?} {line}:{column}"
                    );
                }
            }
        }
    }

    /// QA 19: F8 with 3000 host problems in a 6000-line document froze
    /// 350-1050 ms per press: each problem's offset was found by counting
    /// the lines from the top. The walk is found in one pass over the text.
    #[test]
    fn the_walk_over_thousands_of_problems_in_a_long_text_is_cheap() {
        let text: String = (0..6000)
            .map(|line| format!("kéy{line}: value {line}\n"))
            .collect();
        let problems: Vec<Diagnostic> = (0..3000)
            .map(|index| Diagnostic {
                line: Some(2 + (index * 2) % 5990),
                column: Some(3),
                end_column: Some(5),
                message: format!("host {index}"),
                ..Diagnostic::default()
            })
            .collect();
        let (took, found) = crate::editor::test_timing::best_of(3, || stops(&text, &problems));
        assert_eq!(found.len(), 3000);
        // Each stop is the problem's own offset (UTF-16), in order.
        let offset = |line: usize| {
            text.split('\n')
                .take(line - 1)
                .map(|text| text.encode_utf16().count() + 1)
                .sum::<usize>()
                + 2
        };
        assert!(
            found
                .iter()
                .all(|stop| stop.offset == Some(offset(problems[stop.index].line.unwrap())))
        );
        assert!(
            found
                .windows(2)
                .all(|pair| pair[0].offset <= pair[1].offset)
        );
        assert!(took.as_millis() < 30, "the walk took {took:?}");
    }
}
