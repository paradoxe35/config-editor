//! The problems covering one place, as the problem tip and the hover card
//! list them: each its own lines (its severity, message, who reported it
//! and its code, its docs link), the most serious first.

use leptos::prelude::*;

use super::super::super::a11y::severity_name;
use super::super::super::diagnostics::{Diagnostic, Severity};
use super::super::super::problem_hit::covering;
use super::super::super::state::EditorState;
use crate::EditorStrings;

/// The entries for the problems covering one-based `line`:`column` (tracks
/// the problems, the texts and the one F8 went to; the text is read
/// untracked: a change to it changes the problems).
pub(crate) fn problems_at(state: EditorState, (line, column): (usize, usize)) -> Vec<Entry> {
    let problems = state.problems();
    let walked = problems.walked.get().map(|(problem, _)| problem);
    state.doc().text.with_untracked(|text| {
        let line_text = text.split('\n').nth(line - 1).unwrap_or("");
        state.config().settings.strings.with(|strings| {
            problems.diagnostics.with(|found| {
                entries_at(strings, found, (line, column, line_text), walked.as_ref())
            })
        })
    })
}

/// One problem in the tip (or in the hover card, at a documented key).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub(crate) severity: Severity,
    pub(crate) severity_name: String,
    pub(crate) message: String,
    /// Who reported it and its code ("burrow · E_PORT"), for a host's.
    pub(crate) detail: Option<String>,
    pub(crate) docs_url: Option<String>,
    /// The problem F8 went to last (scrolled into view).
    pub(crate) current: bool,
}

/// The entries' lines (built once per content: no reactive reads inside).
pub(crate) fn entries_view(state: EditorState, entries: Vec<Entry>) -> impl IntoView {
    let touch = super::super::super::touch::touch_screen();
    let open_docs = state.strings_untracked(|strings| open_docs_text(strings, touch).to_string());
    entries
        .into_iter()
        .map(|entry| {
            let docs = entry.docs_url.clone();
            let link = entry.docs_url.map(|url| {
                view! {
                    <a
                        class="config-editor__problem-tip-link"
                        href=url
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        {open_docs.clone()}
                    </a>
                }
            });
            let has_detail = entry.detail.is_some() || link.is_some();
            let separator = (entry.detail.is_some() && link.is_some()).then_some(" · ");
            view! {
                <div
                    class=format!(
                        "config-editor__problem-tip-entry config-editor__problem-tip-entry--{}",
                        entry.severity.as_str(),
                    )
                    class:config-editor__problem-tip-entry--current=entry.current
                >
                    <p
                        class="config-editor__problem-tip-message"
                        class:config-editor__problem-tip-message--docs=docs.is_some()
                        on:click=move |_| open(docs.as_deref())
                    >
                        <span class="config-editor__problem-tip-severity">{entry.severity_name}</span>
                        " "
                        {entry.message}
                    </p>
                    {has_detail.then(|| view! {
                        <p class="config-editor__problem-tip-detail">{entry.detail}{separator}{link}</p>
                    })}
                </div>
            }
        })
        .collect_view()
}

/// A problem's docs link: it names its key (Alt+Enter), unless on a
/// `touch` screen.
fn open_docs_text(strings: &EditorStrings, touch: bool) -> &str {
    if touch {
        &strings.open_docs_touch
    } else {
        &strings.open_docs
    }
}

/// A click on a message with docs opens them, as its link does.
fn open(url: Option<&str>) {
    if let (Some(url), Some(window)) = (url, web_sys::window()) {
        let _ = window.open_with_url_and_target_and_features(url, "_blank", "noopener");
    }
}

/// Every problem covering one-based `line`:`column` of `line_text` (all of
/// them, drawn or past the underline cap, the outer ones of nested problems
/// too), the most serious first; `walked` is the one F8 went to.
pub(crate) fn entries_at(
    strings: &EditorStrings,
    problems: &[Diagnostic],
    (line, column, line_text): (usize, usize, &str),
    walked: Option<&Diagnostic>,
) -> Vec<Entry> {
    let mut here: Vec<&Diagnostic> = covering(problems, line, column, line_text).collect();
    here.sort_by_key(|problem| problem.severity as u8);
    here.into_iter()
        .map(|problem| Entry {
            severity: problem.severity,
            severity_name: severity_name(strings, problem.severity).to_string(),
            message: problem.message.clone(),
            detail: reported_by(problem),
            docs_url: problem.docs_url.clone(),
            current: walked == Some(problem),
        })
        .collect()
}

/// "burrow · E_PORT": who reported a host's problem, and its code.
fn reported_by(problem: &Diagnostic) -> Option<String> {
    if !problem.reported_by_host() {
        return None;
    }
    let parts: Vec<&str> = [problem.source.as_deref(), problem.code.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::entries_at;
    use crate::{Diagnostic, DiagnosticOrigin, EditorStrings, Severity};

    /// QA 19: several problems at one place each keep their own source,
    /// code and docs link, the most serious first (a warning's source was
    /// shown under an error's message).
    #[test]
    fn each_problem_at_one_place_keeps_its_own_detail_most_serious_first() {
        let host =
            |severity, source: Option<&str>, code: Option<&str>, docs: Option<&str>| Diagnostic {
                line: Some(2),
                column: Some(3),
                severity,
                message: format!("{severity:?}"),
                origin: DiagnosticOrigin::Host,
                source: source.map(str::to_string),
                code: code.map(str::to_string),
                docs_url: docs.map(str::to_string),
                ..Diagnostic::default()
            };
        let own = Diagnostic {
            line: Some(2),
            column: Some(3),
            message: "schema".into(),
            ..Diagnostic::default()
        };
        let problems = [
            host(Severity::Warning, Some("lint"), Some("W1"), None),
            host(
                Severity::Hint,
                Some("host"),
                None,
                Some("https://example.com/h"),
            ),
            own.clone(),
            host(
                Severity::Error,
                Some("burrow"),
                Some("E_PORT"),
                Some("https://example.com/e"),
            ),
            Diagnostic {
                line: Some(2),
                column: Some(4),
                ..own
            },
        ];
        let entries = entries_at(
            &EditorStrings::default(),
            &problems,
            (2, 3, "  key: value"),
            Some(&problems[0]),
        );
        let seen: Vec<_> = entries
            .iter()
            .map(|entry| {
                (
                    entry.severity_name.as_str(),
                    entry.message.as_str(),
                    entry.detail.as_deref(),
                    entry.docs_url.as_deref(),
                    entry.current,
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("Error", "schema", None, None, false),
                (
                    "Error",
                    "Error",
                    Some("burrow · E_PORT"),
                    Some("https://example.com/e"),
                    false
                ),
                ("Warning", "Warning", Some("lint · W1"), None, true),
                (
                    "Hint",
                    "Hint",
                    Some("host"),
                    Some("https://example.com/h"),
                    false
                ),
            ]
        );
    }
}
