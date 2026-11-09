//! The merge walking trees of markers (`overlap`) shows what comparing
//! each problem with every other did, on many small random sets of
//! problems sharing a few messages, reporters and lines.

use super::super::diagnostics::{Diagnostic, Severity};
use super::merge;

/// A small random number generator (the same numbers on every run).
struct Numbers(u64);

impl Numbers {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound
    }
}

/// A random problem on the first few lines, maybe over several, maybe
/// with no place.
fn problem(numbers: &mut Numbers, messages: &[&str], host: bool) -> Diagnostic {
    let severities = [
        Severity::Error,
        Severity::Warning,
        Severity::Info,
        Severity::Hint,
    ];
    let line = 1 + numbers.below(4);
    let column = 1 + numbers.below(6);
    let placed = numbers.below(8) > 0;
    let end_line = (numbers.below(3) == 0).then(|| line + numbers.below(3));
    let end_column = (numbers.below(4) > 0).then(|| 1 + numbers.below(8));
    let sources = [None, Some("a"), Some("b")];
    Diagnostic {
        line: placed.then_some(line),
        column: placed.then_some(column),
        end_line,
        end_column,
        message: messages[numbers.below(messages.len())].to_string(),
        severity: severities[numbers.below(4)],
        source: host
            .then(|| sources[numbers.below(3)].map(str::to_string))
            .flatten(),
        code: (numbers.below(3) == 0).then(|| format!("c{}", numbers.below(3))),
        ..Diagnostic::default()
    }
}

#[test]
fn merging_by_tree_shows_what_comparing_each_with_every_other_did() {
    let host_messages = ["deprecated", "too big", " deprecated "];
    let own_messages = ["deprecated", "/a/b: deprecated", "/a: too big", "other"];
    let mut numbers = Numbers(7);
    for round in 0..20_000 {
        let host: Vec<Diagnostic> = (0..numbers.below(12))
            .map(|_| problem(&mut numbers, &host_messages, true))
            .collect();
        let own: Vec<Diagnostic> = (0..numbers.below(8))
            .map(|_| problem(&mut numbers, &own_messages, false))
            .collect();
        assert_eq!(
            merge(&own, &host),
            old::merge(&own, &host),
            "round {round}: {own:#?} and {host:#?}"
        );
    }
}

/// The merge as it was: each problem compared with every other saying
/// the same.
mod old {
    use std::collections::HashMap;

    use super::super::super::diagnostics::Diagnostic;

    /// The editor's `own` problems and the host's, each problem once: where the
    /// host marks the same text (an overlapping marker) with the same message,
    /// the more serious of the two shows (the host's when as serious, as its
    /// text is what the host acts on), so a schema error the host repeats as a
    /// warning stays an error, once, with the host's name, code and docs link.
    /// The host saying one message on the same text more than once (at several
    /// severities) shows it once, the most serious. A different message (a
    /// host error on a line where the schema finds a deprecated key) is
    /// another problem: both show.
    pub(super) fn merge(own: &[Diagnostic], host: &[Diagnostic]) -> Vec<Diagnostic> {
        let mut host = distinct(host);
        // Each marker as `(start, end, severity, index)` by message: one look
        // per problem, not a pass over thousands of the other side's.
        let mut theirs_by_message: HashMap<&str, Vec<Marker>> = HashMap::new();
        for (index, theirs) in host.iter().enumerate() {
            if let Some(marker) = marker(theirs, index) {
                theirs_by_message
                    .entry(theirs.message.trim())
                    .or_default()
                    .push(marker);
            }
        }
        // Each host problem the editor says more seriously: hidden, into the
        // editor's that says it (none when that one is hidden too).
        let mut hidden: Vec<Option<Option<usize>>> = vec![None; host.len()];
        let mut merged: Vec<Diagnostic> = Vec::with_capacity(own.len() + host.len());
        for (index, mine) in own.iter().enumerate() {
            let Some((start, end, severity, _)) = marker(mine, index) else {
                merged.push(mine.clone());
                continue;
            };
            let overlapping = same_messages(&mine.message)
                .filter_map(|message| theirs_by_message.get(message))
                .flatten()
                .filter(|&&(their_start, their_end, ..)| start < their_end && their_start < end);
            let mut repeated = false;
            let mut hides = Vec::new();
            for &(.., theirs, their_index) in overlapping {
                if theirs <= severity {
                    repeated = true;
                } else {
                    hides.push(their_index);
                }
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
        let mut kept: Vec<Diagnostic> = Vec::with_capacity(host.len());
        let mut by_message: HashMap<(&str, Option<&str>), Vec<Marker>> = HashMap::new();
        for problem in host {
            let Some((start, end, severity, _)) = marker(problem, kept.len()) else {
                kept.push(problem.clone());
                continue;
            };
            let said = by_message
                .entry((problem.message.trim(), problem.source.as_deref()))
                .or_default();
            let same = said
                .iter_mut()
                .find(|(their_start, their_end, ..)| start < *their_end && *their_start < end);
            match same {
                Some((.., theirs, index)) => {
                    let other = &mut kept[*index];
                    if severity < *theirs {
                        *theirs = severity;
                        let less = std::mem::replace(other, problem.clone());
                        take_reporter(other, &less);
                    } else {
                        take_reporter(other, problem);
                    }
                }
                None => {
                    said.push((start, end, severity, kept.len()));
                    kept.push(problem.clone());
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

    /// Where a problem's marker starts and ends, its severity (lower is more
    /// serious) and its index in its list.
    type Marker = (Spot, Spot, u8, usize);

    fn marker(problem: &Diagnostic, index: usize) -> Option<Marker> {
        span(problem).map(|(start, end)| (start, end, problem.severity as u8, index))
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
}
