//! QA 23 (M23-U2): a host problem whose range covered one of the editor's
//! own hid it whatever it said (a whole-line or multi-line range, one on
//! the key): the footer, F8 and screen readers never reached the editor's.
//! Only the same message on the same text shows once.

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::paths::{TextPosition, TextRange};
use super::tests::{DOCUMENTS, report};
use super::{HostDiagnostic, RangeText, merge, place};
use crate::strings::default_strings;

const SCHEMA: &str = r#"{"properties":{
    "listen":{"type":"integer","maximum":100},
    "web":{"properties":{"http":{"type":"integer","deprecated":true}}}
}}"#;

/// The host's problems placed in `text`.
fn placed(language: Language, text: &str, host: &[HostDiagnostic]) -> Vec<Diagnostic> {
    place(
        language.behavior(),
        text,
        host,
        RangeText::OnScreen,
        default_strings(),
    )
}

/// The range over the whole of one-based `line` of `text`.
fn whole_line(text: &str, line: usize) -> TextRange {
    let length = text.split('\n').nth(line - 1).unwrap_or("").chars().count();
    TextRange::new(
        TextPosition::new(line, 1),
        TextPosition::new(line, length + 1),
    )
}

/// The range a problem marks.
fn marks(problem: &Diagnostic) -> TextRange {
    let line = problem.line.unwrap();
    TextRange::new(
        TextPosition::new(line, problem.column.unwrap()),
        TextPosition::new(problem.last_line().unwrap(), problem.end_column.unwrap()),
    )
}

/// The message without the JSON pointer a schema message starts with.
fn bare(message: &str) -> &str {
    message.split_once(": ").map_or(message, |(_, rest)| rest)
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_host_problem_hides_the_editors_own_only_when_it_says_the_same() {
    let schema = super::super::Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, text) in DOCUMENTS {
        let own = schema.validate(language, text);
        let Some(too_big) = own.iter().find(|problem| !problem.deprecated) else {
            // XML text is a string the schema cannot compare: nothing to hide.
            if language != Language::XML {
                failures.push(format!("{language}: no maximum problem in {own:?}"));
            }
            continue;
        };
        let deprecated = own.iter().find(|problem| problem.deprecated);
        let Some(deprecated) = deprecated else {
            failures.push(format!("{language}: no deprecation in {own:?}"));
            continue;
        };
        let lines = text.split('\n').count();
        let error = |range: TextRange, message: &str| {
            HostDiagnostic::at_range(range, Severity::Error, message)
        };
        let line = too_big.line.unwrap();
        // (case, host problems, how many of the editor's own still show)
        let cases: Vec<(&str, Vec<HostDiagnostic>, usize)> = vec![
            ("none", vec![], 2),
            (
                "whole line",
                vec![error(whole_line(text, line), "H line")],
                2,
            ),
            (
                "multi-line",
                vec![error(
                    TextRange::new(TextPosition::new(1, 1), TextPosition::new(lines, 1)),
                    "H multi",
                )],
                2,
            ),
            ("on the key", vec![error(marks(deprecated), "H key")], 2),
            (
                "the path",
                vec![HostDiagnostic::at_path(
                    "/listen",
                    Severity::Error,
                    "H path",
                )],
                2,
            ),
            (
                "same message, pointer and all",
                vec![error(whole_line(text, line), &too_big.message)],
                1,
            ),
            (
                "same message at the path",
                vec![HostDiagnostic::at_path(
                    "/listen",
                    Severity::Error,
                    bare(&too_big.message),
                )],
                1,
            ),
            (
                "same deprecation on the key",
                vec![HostDiagnostic::at_range(
                    marks(deprecated),
                    Severity::Warning,
                    &deprecated.message,
                )],
                1,
            ),
            (
                "same message elsewhere",
                vec![error(whole_line(text, lines - 1), &too_big.message)],
                2,
            ),
        ];
        for (case, host, expected) in cases {
            let host = placed(language, text, &host);
            let merged = merge(&own, &host);
            let shown = own.iter().filter(|mine| merged.contains(mine)).count();
            if shown != expected || merged.len() != shown + host.len() {
                failures.push(format!(
                    "{language} {case}: {shown} of the editor's shown, expected {expected}: {merged:?}"
                ));
            }
        }
        // QA 24 (L24-U5): the same message, less serious from the host (a
        // warning for the schema's error) showed twice; the more serious,
        // the editor's, shows once.
        for severity in [Severity::Warning, Severity::Hint] {
            for host in [
                HostDiagnostic::at_path("/listen", severity, bare(&too_big.message)),
                HostDiagnostic::at_range(whole_line(text, line), severity, &too_big.message),
            ] {
                let host = placed(language, text, &[host]);
                let merged = merge(&own, &host);
                if merged != own {
                    failures.push(format!(
                        "{language} same message as a {severity:?}: {merged:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}
