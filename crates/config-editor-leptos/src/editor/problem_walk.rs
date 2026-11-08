//! The order F8 and Shift+F8 walk the problems in: by position, several at
//! one position one after another, then those with no position (a whole
//! document's, a field not found), which the footer shows.

use super::commands::{cycle_offset, offset_at};
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
/// they were found), then the unplaced ones.
pub(super) fn stops(text: &str, problems: &[Diagnostic]) -> Vec<Stop> {
    let mut stops: Vec<Stop> = problems
        .iter()
        .enumerate()
        .filter(|(_, problem)| problem.code.as_deref() != Some(MORE_PROBLEMS_CODE))
        .map(|(index, problem)| Stop {
            offset: problem
                .line
                .zip(problem.column)
                .map(|(line, column)| offset_at(text, line, column)),
            index,
        })
        .collect();
    stops.sort_by_key(|stop| (stop.offset.is_none(), stop.offset, stop.index));
    stops
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
}
