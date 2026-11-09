//! What each host problem is placed by on screen: a path's field, a range
//! of the text on screen, or, while editing as another language, the field
//! of the host's text a range starts in.
//!
//! The fields the ranges start in are read from the host's text, which an
//! edit in the view leaves as it is until it converts back: they are found
//! once for a host text and a list of problems ([`HostTargets`]) and kept
//! while both stay the same, so an edit re-places the problems on the text
//! on screen only (5000 ranges read the host's text again after every key,
//! over a second each time).

use std::sync::Arc;

use leptos::prelude::*;

use super::super::language::ConfigLanguage;
use super::super::paths::{
    TextPosition, TextRange, fields, memo_key_span, offset_of, paths_at, split_line, with_spans,
};
use super::fields::HostFields;
use super::place::RangeText;
use super::{DiagnosticTarget, HostDiagnostic};

/// What a problem is placed by on screen.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Target {
    /// The field at a pointer: on its key when `on_key`, else where a
    /// problem with it is marked.
    Field { pointer: String, on_key: bool },
    /// A range of the text on screen.
    Covered(TextRange),
    /// A place of the text on screen, with no end.
    Point(TextPosition),
    /// Nothing on screen: the footer.
    Nowhere,
}

/// The targets of a list of problems while editing as another language,
/// with the host text and the problems they were found for.
pub(in crate::editor) struct HostTargets {
    language: &'static str,
    text: String,
    problems: Vec<HostDiagnostic>,
    targets: Vec<Target>,
}

impl HostTargets {
    /// The targets of `problems`, their ranges counted in `host`'s `text`
    /// (every range's field found in one read of it).
    pub(in crate::editor) fn new(
        host: &'static dyn ConfigLanguage,
        text: &str,
        problems: &[HostDiagnostic],
    ) -> Self {
        HostTargets {
            language: host.info().label,
            text: text.to_string(),
            problems: problems.to_vec(),
            targets: resolve(host, text, problems),
        }
    }

    /// What each problem is placed by.
    #[cfg(all(
        test,
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    ))]
    pub(super) fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// [`HostTargets::new`], kept in `cache`: the ones found before while
    /// the host's text and the problems are the same.
    pub(in crate::editor) fn cached(
        cache: StoredValue<Option<Arc<HostTargets>>>,
        host: &'static dyn ConfigLanguage,
        text: &str,
        problems: &[HostDiagnostic],
    ) -> Arc<HostTargets> {
        let label = host.info().label;
        let kept = cache.with_value(|kept| {
            kept.as_ref()
                .filter(|kept| {
                    kept.language == label && kept.text == text && kept.problems == problems
                })
                .cloned()
        });
        kept.unwrap_or_else(|| {
            let found = Arc::new(HostTargets::new(host, text, problems));
            cache.set_value(Some(Arc::clone(&found)));
            found
        })
    }
}

/// What each of `problems` is placed by: a path's field; a range, where
/// the text on screen is the host's, else the field it starts in.
pub(super) fn targets(problems: &[HostDiagnostic], ranges: RangeText<'_>) -> Vec<Target> {
    match ranges {
        RangeText::OnScreen => problems
            .iter()
            .map(|problem| match &problem.target {
                DiagnosticTarget::Range(range) => Target::Covered(*range),
                DiagnosticTarget::Position(position) => Target::Point(*position),
                _ => of_path(problem),
            })
            .collect(),
        RangeText::Host(found) => {
            debug_assert!(found.problems == problems, "targets of other problems");
            found.targets.clone()
        }
    }
}

fn of_path(problem: &HostDiagnostic) -> Target {
    match &problem.target {
        DiagnosticTarget::Path(path) if !path.is_root() => Target::Field {
            pointer: path.to_pointer(),
            on_key: false,
        },
        _ => Target::Nowhere,
    }
}

/// [`targets`] with the ranges counted in `host`'s `text`.
fn resolve(
    host: &'static dyn ConfigLanguage,
    text: &str,
    problems: &[HostDiagnostic],
) -> Vec<Target> {
    let has_ranges = problems
        .iter()
        .any(|problem| starting(&problem.target).is_some());
    if !has_ranges {
        return problems.iter().map(of_path).collect();
    }
    let host_fields = host
        .parse(text)
        .map(|document| fields(&document))
        .unwrap_or_default();
    let pointers: Vec<String> = host_fields
        .iter()
        .map(|(pointer, _)| pointer.clone())
        .collect();
    with_spans(host, text, &pointers, || {
        let written = HostFields::new(host, text, host_fields);
        let starts: Vec<Option<Start>> = problems
            .iter()
            .map(|problem| {
                starting(&problem.target).and_then(|range| start_of(text, &written, range))
            })
            .collect();
        // The places no field is written at, read in one pass of the text.
        let line_breaks_only = !text.contains('\r');
        let unwritten: Vec<usize> = starts
            .iter()
            .flatten()
            .filter(|start| start.written.is_none())
            .map(|start| offset_of(text, start.position, line_breaks_only))
            .collect();
        let mut read = paths_at(host, text, &unwritten).into_iter();
        problems
            .iter()
            .zip(starts)
            .map(
                |(problem, start)| match (starting(&problem.target), start) {
                    (Some(_), Some(start)) => {
                        let pointer = match start.written {
                            Some(pointer) => Some(pointer),
                            None => read
                                .next()
                                .flatten()
                                .filter(|path| !path.is_root())
                                .map(|path| path.to_pointer()),
                        };
                        pointer.map_or(Target::Nowhere, |pointer| {
                            let on_key = memo_key_span(host, text, &pointer).is_some_and(|key| {
                                key.line == start.position.line
                                    && (key.start_column..key.end_column)
                                        .contains(&start.position.column)
                            });
                            Target::Field { pointer, on_key }
                        })
                    }
                    (Some(_), None) => Target::Nowhere,
                    _ => of_path(problem),
                },
            )
            .collect()
    })
}

/// The range a target in the host's text starts: a range's own, a
/// point's (empty) at it; `None` for a path or the document.
fn starting(target: &DiagnosticTarget) -> Option<TextRange> {
    match target {
        DiagnosticTarget::Range(range) => Some(*range),
        DiagnosticTarget::Position(position) => Some(TextRange::new(*position, *position)),
        _ => None,
    }
}

/// Where a range starts in the host's text, and the field written there
/// when one is.
struct Start {
    position: TextPosition,
    written: Option<String>,
}

/// Where `range` starts in the host's `text`: in the line's indentation (a
/// whole line selected from its start), at the field written on that line;
/// with the field whose key or value is written there, if one is (else
/// only reading the text tells). `None` past the text.
fn start_of(text: &str, written: &HostFields, range: TextRange) -> Option<Start> {
    let line_text = split_line(text, range.start.line.saturating_sub(1))?;
    let mut position = range.start;
    let indent = line_text
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    if position.column <= indent && indent < line_text.chars().count() {
        position.column = indent + 1;
    }
    Some(Start {
        position,
        written: written.at(position).map(str::to_string),
    })
}
