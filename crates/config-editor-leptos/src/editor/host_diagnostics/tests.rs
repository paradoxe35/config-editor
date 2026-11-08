//! Host diagnostics placed in each language's version of one document, and
//! merged with the editor's own. Each test lists every failing case at once.

use super::super::Language;
use super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use super::super::paths::{TextPosition, TextRange};
use super::{HostDiagnostic, RangeText, merge};
use crate::strings::default_strings;

/// [`super::place`] with the default texts.
fn place(
    behavior: &dyn super::super::language::ConfigLanguage,
    source: &str,
    problems: &[HostDiagnostic],
    on_screen: bool,
) -> Vec<Diagnostic> {
    let ranges = if on_screen {
        RangeText::OnScreen
    } else {
        // No host text holds a field at any range.
        RangeText::Host(Language::YAML.behavior(), "")
    };
    super::place(behavior, source, problems, ranges, default_strings())
}

pub(super) const DOCUMENTS: [(Language, &str); 5] = [
    (
        Language::YAML,
        "listen: 7835\nweb:\n  http: 80\n  hosts:\n    - a.example\n    - b.example\n",
    ),
    (
        Language::JSON,
        "{\n  \"listen\": 7835,\n  \"web\": {\n    \"http\": 80,\n    \"hosts\": [\"a.example\", \"b.example\"]\n  }\n}\n",
    ),
    (
        Language::TOML,
        "listen = 7835\n\n[web]\nhttp = 80\nhosts = [\"a.example\", \"b.example\"]\n",
    ),
    (
        Language::HCL,
        "listen = 7835\nweb {\n  http = 80\n  hosts = [\"a.example\", \"b.example\"]\n}\n",
    ),
    (
        Language::XML,
        "<listen>7835</listen>\n<web>\n  <http>80</http>\n  <hosts>a.example</hosts>\n  <hosts>b.example</hosts>\n</web>\n",
    ),
];

/// The text a diagnostic underlines.
pub(super) fn marked(text: &str, diagnostic: &Diagnostic) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (line, column) = (diagnostic.line?, diagnostic.column?);
    let (end_line, end) = (diagnostic.last_line()?, diagnostic.end_column?);
    let covered: Vec<String> = (line..=end_line)
        .map(|at| {
            let chars: Vec<char> = lines.get(at - 1).copied().unwrap_or("").chars().collect();
            let from = if at == line { column - 1 } else { 0 };
            let to = if at == end_line { end - 1 } else { chars.len() };
            chars[from.min(chars.len())..to.min(chars.len()).max(from.min(chars.len()))]
                .iter()
                .collect()
        })
        .collect();
    Some(covered.join("\n"))
}

#[track_caller]
pub(super) fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn paths_are_marked_where_the_field_is_written_in_every_language() {
    // (path, what is underlined per language: YAML, JSON, TOML, HCL, XML)
    let cases: [(&str, [Option<&str>; 5]); 5] = [
        ("/web/http", [Some("80"); 5]),
        ("/listen", [Some("7835"); 5]),
        (
            "/web/hosts/1",
            [
                Some("b.example"),
                Some("\"b.example\""),
                Some("\"b.example\""),
                Some("\"b.example\""),
                Some("b.example"),
            ],
        ),
        // A block is marked on its key, never on the brace opening it (QA 18:
        // JSON underlined only `{`).
        (
            "/web",
            [
                Some("web"),
                Some("\"web\""),
                Some("[web]"),
                Some("web"),
                Some("web"),
            ],
        ),
        ("/nope", [None; 5]),
    ];
    let mut failures = Vec::new();
    for (pointer, expected) in cases {
        for ((language, text), expected) in DOCUMENTS.into_iter().zip(expected) {
            let problems = [HostDiagnostic::at_path(pointer, Severity::Error, "bad")];
            let placed = place(language.behavior(), text, &problems, true);
            let found = placed.first().and_then(|problem| marked(text, problem));
            if placed.len() != 1 || found.as_deref() != expected {
                failures.push(format!(
                    "{language} {pointer}: expected {expected:?}, found {found:?} ({} placed)",
                    placed.len()
                ));
            }
        }
    }
    report(failures);
}

#[test]
fn host_problems_keep_their_details_and_show_once() {
    let (language, text) = DOCUMENTS[0];
    let problem = HostDiagnostic::at_path("/listen", Severity::Warning, "port in use")
        .with_code("E1")
        .with_docs_url("https://example.com/e1");
    let placed = place(
        language.behavior(),
        text,
        &[problem.clone(), problem.with_source("burrow")],
        true,
    );
    assert_eq!(placed.len(), 2, "{placed:?}");
    let first = &placed[0];
    assert_eq!(first.origin, DiagnosticOrigin::Host);
    assert_eq!(first.severity, Severity::Warning);
    assert_eq!(first.code.as_deref(), Some("E1"));
    assert_eq!(first.source.as_deref(), Some("host"));
    assert_eq!(first.docs_url.as_deref(), Some("https://example.com/e1"));
    assert_eq!(placed[1].source.as_deref(), Some("burrow"));
    let same = HostDiagnostic::at_path("/listen", Severity::Info, "note");
    let twice = place(language.behavior(), text, &[same.clone(), same], true);
    assert_eq!(twice.len(), 1);
}

#[test]
fn ranges_are_marked_only_on_the_host_text() {
    let text = "listen: 7835\nweb:\n  http: 80\n";
    let range = |line, column, end_line, end_column| {
        TextRange::new(
            TextPosition::new(line, column),
            TextPosition::new(end_line, end_column),
        )
    };
    let cases = [
        (range(1, 9, 1, 13), Some("7835")),
        // Over several lines: from the start on the first (the end line
        // keeps the rest, see `range_tests`).
        (range(2, 1, 3, 5), Some("web:\n  ht")),
        // Empty: one character.
        (range(3, 3, 3, 3), Some("h")),
        // Past the end of its line: clamped.
        (range(1, 9, 1, 99), Some("7835")),
        (range(9, 1, 9, 2), None),
    ];
    let mut failures = Vec::new();
    for (range, expected) in cases {
        let problems = [HostDiagnostic::at_range(range, Severity::Hint, "x")];
        let placed = place(Language::YAML.behavior(), text, &problems, true);
        let found = marked(text, &placed[0]);
        if found.as_deref() != expected {
            failures.push(format!("{range:?}: expected {expected:?}, found {found:?}"));
        }
        let elsewhere = place(Language::YAML.behavior(), text, &problems, false);
        if elsewhere[0].line.is_some() {
            failures.push(format!("{range:?}: placed on another language's text"));
        }
    }
    report(failures);
}

#[test]
fn the_host_problem_replaces_the_editors_own_on_the_same_text() {
    let own = |line, column, end, message: &str| Diagnostic {
        line: Some(line),
        column: Some(column),
        end_column: Some(end),
        message: message.to_string(),
        ..Diagnostic::default()
    };
    let host = Diagnostic {
        origin: DiagnosticOrigin::Host,
        ..own(1, 9, 13, "from the host")
    };
    let mine = [
        own(1, 10, 12, "overlapping"),
        own(1, 1, 7, "before it on the line"),
        own(2, 9, 13, "another line"),
        Diagnostic {
            line: None,
            column: None,
            end_column: None,
            ..own(1, 1, 1, "unplaced")
        },
    ];
    let merged: Vec<String> = merge(&mine, std::slice::from_ref(&host))
        .into_iter()
        .map(|problem| problem.message)
        .collect();
    assert_eq!(
        merged,
        [
            "before it on the line",
            "another line",
            "unplaced",
            "from the host"
        ]
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_problem_both_the_schema_and_the_host_find_shows_once_in_every_language() {
    let schema = super::super::Schema::compile(
        r#"{"properties":{"web":{"properties":{"http":{"type":"string"}}}}}"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for (language, text) in DOCUMENTS {
        let own = schema.validate(language, text);
        let host = place(
            language.behavior(),
            text,
            &[HostDiagnostic::at_path(
                "/web/http",
                Severity::Error,
                "http must be a name",
            )],
            true,
        );
        let merged = merge(&own, &host);
        let messages: Vec<&str> = merged
            .iter()
            .map(|problem| problem.message.as_str())
            .collect();
        // XML text reads as the string the schema asks for: no problem of its own.
        let expected_own = usize::from(language != Language::XML);
        if own.len() != expected_own || messages != ["http must be a name"] {
            failures.push(format!("{language}: own {own:?}, merged {messages:?}"));
        }
    }
    report(failures);
}

#[test]
fn whole_document_and_root_problems_are_unplaced() {
    let (language, text) = DOCUMENTS[1];
    let problems = [
        HostDiagnostic::document(Severity::Error, "server rejected it"),
        HostDiagnostic::at_path("", Severity::Error, "root"),
    ];
    let placed = place(language.behavior(), text, &problems, true);
    assert!(
        placed.iter().all(|problem| problem.line.is_none()),
        "{placed:?}"
    );
}

#[test]
fn unplaced_problems_at_a_field_name_its_path_in_every_language() {
    let at = |path: &str, severity, message: &str| HostDiagnostic::at_path(path, severity, message);
    let problems = [
        at("/nope", Severity::Error, "must be set"),
        at("/other/x", Severity::Error, "must be set"),
        at("/nope", Severity::Warning, "must be set"),
        // Placed ones are at their field: the message as the host wrote it.
        at("/listen", Severity::Error, "must be set"),
        // The root and the whole document have no path to name.
        at("", Severity::Error, "root problem"),
        HostDiagnostic::document(Severity::Error, "rejected"),
    ];
    let mut failures = Vec::new();
    for (language, text) in DOCUMENTS {
        let placed = place(language.behavior(), text, &problems, true);
        let shown: Vec<(bool, &str)> = placed
            .iter()
            .map(|problem| (problem.line.is_some(), problem.message.as_str()))
            .collect();
        let expected = [
            (false, "/nope: must be set"),
            (false, "/other/x: must be set"),
            (false, "/nope: must be set"),
            (true, "must be set"),
            (false, "root problem"),
            (false, "rejected"),
        ];
        if shown != expected {
            failures.push(format!("{language}: {shown:?}"));
        }
    }
    report(failures);
}
