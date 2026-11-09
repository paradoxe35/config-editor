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
use super::super::paths::{TextRange, memo_key_span, path_at, split_line, with_spans};
use super::fields::{HostFields, fields};
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
        .any(|problem| matches!(problem.target, DiagnosticTarget::Range(_)));
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
        problems
            .iter()
            .map(|problem| match &problem.target {
                DiagnosticTarget::Range(range) => field_of(host, text, &written, *range).map_or(
                    Target::Nowhere,
                    |(pointer, on_key)| Target::Field { pointer, on_key },
                ),
                _ => of_path(problem),
            })
            .collect()
    })
}

/// The field of the host's `text` that `range` starts in, and whether it
/// starts on its key: looked up where the fields are `written` when it
/// starts on a key or a value, else read from the text. `None` when it
/// starts in no field (past the text, or at the top level).
fn field_of(
    host: &'static dyn ConfigLanguage,
    text: &str,
    written: &HostFields,
    range: TextRange,
) -> Option<(String, bool)> {
    let line_text = split_line(text, range.start.line.saturating_sub(1))?;
    // Starting in the line's indentation (a whole line selected from its
    // start), it starts at the field written on that line.
    let mut start = range.start;
    let indent = line_text
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    if start.column <= indent && indent < line_text.chars().count() {
        start.column = indent + 1;
    }
    let pointer = match written.at(start) {
        Some(pointer) => pointer.to_string(),
        None => {
            let path = path_at(host, text, start.to_offset(text))?;
            if path.is_root() {
                return None;
            }
            path.to_pointer()
        }
    };
    let on_key = memo_key_span(host, text, &pointer).is_some_and(|key| {
        key.line == start.line && (key.start_column..key.end_column).contains(&start.column)
    });
    Some((pointer, on_key))
}
