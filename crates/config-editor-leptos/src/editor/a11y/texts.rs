//! How keyboard and screen-reader access words problems, from the
//! `strings` setting.

use super::super::completion::{ChoiceHover, KeyHover};
use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use crate::EditorStrings;
use crate::strings::fill;

/// "Error", "Warning", "Info", "Hint".
pub(in crate::editor) fn severity_name(strings: &EditorStrings, severity: Severity) -> &str {
    match severity {
        Severity::Error => &strings.error,
        Severity::Warning => &strings.warning,
        Severity::Info => &strings.info,
        Severity::Hint => &strings.hint,
    }
}

/// " from burrow" for a host problem that names its source, else nothing.
fn source_part(strings: &EditorStrings, problem: &Diagnostic) -> String {
    problem
        .source
        .as_deref()
        .filter(|_| problem.origin == DiagnosticOrigin::Host)
        .map(|source| fill(&strings.problem_source, &[("source", &source)]))
        .unwrap_or_default()
}

/// " Code E12. Documentation: https://… (Alt+Enter opens it).": a host
/// problem's code and docs link, which the pointer sees under its message.
fn links_part(strings: &EditorStrings, problem: &Diagnostic) -> String {
    let code = problem
        .code
        .as_deref()
        .filter(|_| problem.origin == DiagnosticOrigin::Host)
        .map(|code| fill(&strings.problem_code, &[("code", &code)]));
    let docs = problem
        .docs_url
        .as_deref()
        .map(|url| fill(&strings.problem_docs, &[("url", &url)]));
    code.into_iter().chain(docs).collect()
}

/// `text` and then the problem's code and docs link, as sentences of their
/// own after a message that does not end one.
fn with_links(strings: &EditorStrings, problem: &Diagnostic, text: String) -> String {
    let links = links_part(strings, problem);
    let ended = text.trim_end().ends_with(['.', '!', '?']);
    let stop = if links.is_empty() || ended { "" } else { "." };
    text + stop + &links
}

/// "Error: …", "Warning from burrow: …": a problem as a screen reader
/// hears it, with its code and docs link.
pub(super) fn problem_text(strings: &EditorStrings, problem: &Diagnostic) -> String {
    let text = fill(
        &strings.problem,
        &[
            ("severity", &severity_name(strings, problem.severity)),
            ("source", &source_part(strings, problem)),
            ("message", &problem.message),
        ],
    );
    with_links(strings, problem, text)
}

/// "Error 2 of 5: …": the problem F8 moved to, of the `total` placed ones.
pub(in crate::editor) fn problem_announcement(
    strings: &EditorStrings,
    problem: &Diagnostic,
    position: usize,
    total: usize,
) -> String {
    let text = fill(
        &strings.problem_position,
        &[
            ("severity", &severity_name(strings, problem.severity)),
            ("position", &position),
            ("total", &total),
            ("source", &source_part(strings, problem)),
            ("message", &problem.message),
        ],
    );
    with_links(strings, problem, text)
}

/// "YAML Error on line 3: …": an underlined problem's accessible name.
pub(in crate::editor) fn problem_marker(
    strings: &EditorStrings,
    language: &str,
    problem: &Diagnostic,
    line: usize,
) -> String {
    fill(
        &strings.problem_marker,
        &[
            ("language", &language),
            ("severity", &severity_name(strings, problem.severity)),
            ("source", &source_part(strings, problem)),
            ("line", &line),
            ("message", &problem.message),
        ],
    )
}

/// The hover card's docs as one text, for screen readers when the card is
/// shown from the keyboard or a tap: the value's, then the key's name, type
/// and marks, its description, its `oneOf` option, default and examples.
pub(in crate::editor) fn hover_text(strings: &EditorStrings, hover: &KeyHover) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(value) = &hover.value {
        parts.extend(
            [
                Some(&value.label),
                value.doc.title.as_ref(),
                value.doc.description.as_ref(),
            ]
            .into_iter()
            .flatten()
            .cloned(),
        );
    }
    let mut key = format!("{} ({})", hover.key, hover.type_label);
    for (on, mark) in [
        (hover.required, &strings.required),
        (hover.deprecated, &strings.deprecated),
    ] {
        if on {
            key = format!("{key}, {mark}");
        }
    }
    parts.push(key);
    parts.extend(hover.deprecation.clone());
    parts.extend(hover.description.clone());
    match &hover.choice {
        Some(ChoiceHover::Matched(doc)) => {
            parts.extend(doc.title.clone());
            parts.extend(doc.description.clone());
        }
        Some(ChoiceHover::OneOf(titles)) => {
            parts.push(format!("{}: {}", strings.one_of, titles.join(", ")));
        }
        None => {}
    }
    if let Some(default) = &hover.default {
        parts.push(format!("{}: {default}", strings.default_value));
    }
    if !hover.examples.is_empty() {
        parts.push(format!(
            "{}: {}",
            strings.examples,
            hover.examples.join(", ")
        ));
    }
    parts.join(". ")
}
