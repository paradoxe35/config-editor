//! The editor's problems and the host's, merged: a problem both report
//! shows once.

use std::collections::HashMap;

use super::super::diagnostics::Diagnostic;

/// The editor's `own` problems and the host's, each problem once: where the
/// host marks the same text (an overlapping marker) with the same message,
/// the more serious of the two shows (the host's when as serious, as its
/// text is what the host acts on), so a schema error the host repeats as a
/// warning stays an error, once. A different message (a host error on a
/// line where the schema finds a deprecated key) is another problem: both
/// show.
pub(in super::super) fn merge(own: &[Diagnostic], host: &[Diagnostic]) -> Vec<Diagnostic> {
    // Each marker as `(start, end, severity)` by message: one look per
    // problem, not a pass over thousands of the other side's.
    let mut theirs_by_message: HashMap<&str, Vec<Marker>> = HashMap::new();
    for theirs in host {
        if let Some(marker) = marker(theirs) {
            theirs_by_message
                .entry(theirs.message.trim())
                .or_default()
                .push(marker);
        }
    }
    let mut mine_by_message: HashMap<&str, Vec<Marker>> = HashMap::new();
    for mine in own {
        if let Some(marker) = marker(mine) {
            for message in same_messages(&mine.message) {
                mine_by_message.entry(message).or_default().push(marker);
            }
        }
    }
    // Whether `problem`, saying one of `messages`, is repeated in `others`
    // by one as serious as it (`or_more`), or more serious.
    let repeated = |problem: &Diagnostic,
                    messages: &[&str],
                    others: &HashMap<&str, Vec<Marker>>,
                    or_more: bool| {
        let Some((start, end, severity)) = marker(problem) else {
            return false;
        };
        messages.iter().any(|message| {
            others.get(message).is_some_and(|markers| {
                markers.iter().any(|&(their_start, their_end, theirs)| {
                    let serious = if or_more {
                        theirs <= severity
                    } else {
                        theirs < severity
                    };
                    start < their_end && their_start < end && serious
                })
            })
        })
    };
    let own_shown = own.iter().filter(|mine| {
        let messages: Vec<&str> = same_messages(&mine.message).collect();
        !repeated(mine, &messages, &theirs_by_message, true)
    });
    let host_shown = host
        .iter()
        .filter(|theirs| !repeated(theirs, &[theirs.message.trim()], &mine_by_message, false));
    own_shown.chain(host_shown).cloned().collect()
}

/// Where a problem's marker starts and ends, and its severity (lower is
/// more serious).
type Marker = (Spot, Spot, u8);

fn marker(problem: &Diagnostic) -> Option<Marker> {
    span(problem).map(|(start, end)| (start, end, problem.severity as u8))
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
