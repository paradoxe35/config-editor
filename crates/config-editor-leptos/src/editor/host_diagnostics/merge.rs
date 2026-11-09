//! The editor's problems and the host's, merged: a problem both report
//! shows once.

use super::super::diagnostics::Diagnostic;
use super::overlap::{Grouped, Span, by_key};

/// The editor's `own` problems and the host's, each problem once: where the
/// host marks the same text (an overlapping marker) with the same message,
/// the more serious of the two shows (the host's when as serious, as its
/// text is what the host acts on), so a schema error the host repeats as a
/// warning stays an error, once, with the host's name, code and docs link.
/// The host saying one message on the same text more than once (at several
/// severities) shows it once, the most serious. A different message (a
/// host error on a line where the schema finds a deprecated key) is
/// another problem: both show.
pub(in super::super) fn merge(own: &[Diagnostic], host: &[Diagnostic]) -> Vec<Diagnostic> {
    let mut host = distinct(host);
    // The host's markers in a tree for each message: one walk per problem
    // to the ones it overlaps, not a look at each of thousands.
    let spans: Vec<Option<Span>> = host.iter().map(span).collect();
    let mut grouped = by_key(&spans, |index| host[index].message.trim());
    for (index, placed) in grouped.placed.iter().enumerate() {
        if let Some((tree, rank)) = *placed {
            grouped.trees[tree].add(rank, index);
        }
    }
    // Each host problem the editor says more seriously: hidden, into the
    // editor's that says it (none when that one is hidden too).
    let mut hidden: Vec<Option<Option<usize>>> = vec![None; host.len()];
    let mut merged: Vec<Diagnostic> = Vec::with_capacity(own.len() + host.len());
    for mine in own {
        let Some(marker) = span(mine) else {
            merged.push(mine.clone());
            continue;
        };
        let severity = mine.severity as u8;
        let mut repeated = false;
        let mut hides = Vec::new();
        for message in same_messages(&mine.message) {
            let Some(&tree) = grouped.trees_by_key.get(message) else {
                continue;
            };
            grouped.trees[tree].each(marker, |their_index| {
                if host[their_index].severity as u8 <= severity {
                    repeated = true;
                } else {
                    hides.push(their_index);
                }
            });
        }
        let into = (!repeated).then_some(merged.len());
        for their_index in hides {
            let fate = &mut hidden[their_index];
            if fate.is_none() || *fate == Some(None) {
                *fate = Some(into);
            }
        }
        if !repeated {
            merged.push(mine.clone());
        }
    }
    for (theirs, fate) in host.iter_mut().zip(hidden) {
        match fate {
            None => merged.push(std::mem::take(theirs)),
            Some(Some(into)) => take_reporter(&mut merged[into], theirs),
            Some(None) => {}
        }
    }
    merged
}

/// The host's problems, one message on the same text from the same
/// reporter once: the most serious (the first of those as serious), with
/// a code or docs link only another of them gave.
fn distinct(host: &[Diagnostic]) -> Vec<Diagnostic> {
    let spans: Vec<Option<Span>> = host.iter().map(span).collect();
    let Grouped {
        mut trees, placed, ..
    } = by_key(&spans, |index| {
        (host[index].message.trim(), host[index].source.as_deref())
    });
    let mut kept: Vec<Diagnostic> = Vec::with_capacity(host.len());
    // The severity of each kept problem (as kept, the most serious).
    let mut severities: Vec<u8> = Vec::with_capacity(host.len());
    for ((problem, span), placed) in host.iter().zip(spans).zip(placed) {
        let severity = problem.severity as u8;
        let (Some(span), Some((tree, rank))) = (span, placed) else {
            kept.push(problem.clone());
            severities.push(severity);
            continue;
        };
        // The kept ones never overlap each other: one that would is kept
        // into the first it overlaps.
        match trees[tree].first_of_apart(span) {
            Some(index) => {
                let other = &mut kept[index];
                if severity < severities[index] {
                    severities[index] = severity;
                    let less = std::mem::replace(other, problem.clone());
                    take_reporter(other, &less);
                } else {
                    take_reporter(other, problem);
                }
            }
            None => {
                trees[tree].add(rank, kept.len());
                kept.push(problem.clone());
                severities.push(severity);
            }
        }
    }
    kept
}

/// `shown` says what `hidden` does: it takes the host's name, code and
/// docs link it lacks.
fn take_reporter(shown: &mut Diagnostic, hidden: &Diagnostic) {
    for (mine, theirs) in [
        (&mut shown.source, &hidden.source),
        (&mut shown.code, &hidden.code),
        (&mut shown.docs_url, &hidden.docs_url),
    ] {
        if mine.is_none() {
            mine.clone_from(theirs);
        }
    }
}

/// Where a problem's marker starts and ends (past its last character, at
/// least one character on: one ending on a line before its own ends on
/// its own).
fn span(problem: &Diagnostic) -> Option<Span> {
    let (line, column) = (problem.line?, problem.column?);
    let last_line = problem.last_line().unwrap_or(line).max(line);
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
