//! The editor's problems and the host's, merged: a problem both report
//! shows once.

use std::collections::HashMap;

use super::super::diagnostics::Diagnostic;

/// The editor's `own` problems and the host's, each problem once: where the
/// host marks the same text (an overlapping marker) with the same message,
/// as serious or more, only the host's shows, as its text is what the host
/// acts on. A different message (a host error on a line where the schema
/// finds a deprecated key) is another problem, and a less serious one (a
/// hint on a field the schema finds wrong) never hides the editor's: both
/// show.
pub(in super::super) fn merge(own: &[Diagnostic], host: &[Diagnostic]) -> Vec<Diagnostic> {
    // Each marker as `(start, end, severity)` by message: one look per own
    // problem, not a pass over thousands of the host's.
    let mut by_message: HashMap<&str, Vec<(Spot, Spot, u8)>> = HashMap::new();
    for theirs in host {
        if let Some((start, end)) = span(theirs) {
            by_message.entry(theirs.message.trim()).or_default().push((
                start,
                end,
                theirs.severity as u8,
            ));
        }
    }
    let repeated = |mine: &Diagnostic| {
        let Some((start, end)) = span(mine) else {
            return false;
        };
        same_messages(&mine.message).any(|message| {
            by_message.get(message).is_some_and(|markers| {
                markers.iter().any(|&(their_start, their_end, severity)| {
                    start < their_end && their_start < end && severity <= mine.severity as u8
                })
            })
        })
    };
    own.iter()
        .filter(|mine| !repeated(mine))
        .chain(host)
        .cloned()
        .collect()
}

/// A one-based `(line, column)` place.
type Spot = (usize, usize);

/// Where a problem's marker starts and ends (past its last character, at
/// least one character on).
fn span(problem: &Diagnostic) -> Option<(Spot, Spot)> {
    let (line, column) = (problem.line?, problem.column?);
    let last_line = problem.last_line().unwrap_or(line);
    let end_column = problem.end_column.unwrap_or(column + 1);
    let end = if last_line == line {
        (line, end_column.max(column + 1))
    } else {
        (last_line, end_column)
    };
    Some(((line, column), end))
}

/// The host messages that say the same as the editor's `message`: it
/// whole, or without the JSON pointer a schema message starts with
/// (`/port: 1 is less than the minimum of 5`).
fn same_messages(message: &str) -> impl Iterator<Item = &str> {
    let message = message.trim();
    let bare = message
        .starts_with('/')
        .then(|| message.split_once(": ").map(|(_, rest)| rest.trim()))
        .flatten();
    std::iter::once(message).chain(bare)
}
