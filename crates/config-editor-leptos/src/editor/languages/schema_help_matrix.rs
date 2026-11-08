//! Richer schema help, checked in every language against the schema-help
//! fixture: deprecated keys, docs links, per-value descriptions, required
//! children filled from examples with Tab stops, and patterns the regex
//! engine cannot read. Each test lists every failing case at once.
//!
//! `|` marks the caret.

use super::super::completion::{
    Completion, apply_completion, completions, key_hover, next_stop, stop_ranges,
};
use super::super::diagnostics::{Diagnostic, SchemaBundle, Severity, build_schema, validate};
use super::Language;

fn help() -> SchemaBundle {
    build_schema(include_str!(
        "../../../tests/fixtures/schema-help.schema.json"
    ))
    .unwrap()
}

/// The options offered at the caret (as after Ctrl+Space).
fn offered(language: Language, marked: &str) -> Vec<Completion> {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&help()),
        true,
        language.behavior(),
    )
}

/// Where a new top-level key is typed, per language.
const ROOT: [(Language, &str); 5] = [
    (Language::YAML, "|\n"),
    (Language::JSON, "{\n  |\n}\n"),
    (Language::TOML, "|\n"),
    (Language::HCL, "|\n"),
    (Language::XML, "<|\n"),
];

#[test]
fn deprecated_keys_are_marked_and_listed_after_the_others() {
    let mut failures = Vec::new();
    for (language, marked) in ROOT {
        let options = offered(language, marked);
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        let deprecated: Vec<&str> = options
            .iter()
            .filter(|option| option.deprecated)
            .map(|option| option.label.as_str())
            .collect();
        if deprecated != ["timeout"] || labels.last() != Some(&"timeout") {
            failures.push(format!(
                "{language:?}: {labels:?}, deprecated {deprecated:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn values_show_their_own_title_and_description() {
    let cases: &[(Language, &str, &str, &str)] = &[
        (Language::YAML, "mode: |\n", "fast", "Skips the checks."),
        (
            Language::JSON,
            "{\"mode\": |}\n",
            "safe",
            "Checks everything.",
        ),
        (Language::TOML, "mode = |\n", "debug", "Logs every step."),
        (Language::HCL, "mode = |\n", "fast", "Skips the checks."),
        (
            Language::XML,
            "<mode>|</mode>\n",
            "safe",
            "Checks everything.",
        ),
        (
            Language::YAML,
            "level: |\n",
            "trace",
            "Trace — Everything, very verbose.",
        ),
        (
            Language::JSON,
            "{\"level\": |}\n",
            "info",
            "Info — Normal messages.",
        ),
        (
            Language::TOML,
            "level = |\n",
            "trace",
            "Trace — Everything, very verbose.",
        ),
        (
            Language::HCL,
            "level = |\n",
            "info",
            "Info — Normal messages.",
        ),
        (
            Language::XML,
            "<level>|</level>\n",
            "trace",
            "Trace — Everything, very verbose.",
        ),
    ];
    let mut failures = Vec::new();
    for &(language, marked, label, detail) in cases {
        let options = offered(language, marked);
        let found = options.iter().find(|option| option.label == label);
        if found.map(|option| option.detail.as_str()) != Some(detail) {
            let all: Vec<(&str, &str)> = options
                .iter()
                .map(|option| (option.label.as_str(), option.detail.as_str()))
                .collect();
            failures.push(format!("{language:?} {marked:?} {label}: {all:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A document with `mode: fast` and `timeout: 3`, the zero-based line and
/// column of the `fast` value, and of the `timeout` key.
const DOCUMENTS: [(Language, &str); 5] = [
    (Language::YAML, "mode: fast\ntimeout: 3\n"),
    (
        Language::JSON,
        "{\n  \"mode\": \"fast\",\n  \"timeout\": 3\n}\n",
    ),
    (Language::TOML, "mode = \"fast\"\ntimeout = 3\n"),
    (Language::HCL, "mode = \"fast\"\ntimeout = 3\n"),
    (Language::XML, "<mode>fast</mode>\n<timeout>3</timeout>\n"),
];

/// The text a diagnostic underlines (on its one line).
fn marked(source: &str, diagnostic: &Diagnostic) -> Option<String> {
    let line = source.split('\n').nth(diagnostic.line? - 1)?;
    let column = diagnostic.column?;
    let end = diagnostic.end_column?;
    Some(line.chars().skip(column - 1).take(end - column).collect())
}

fn position_of(source: &str, needle: &str) -> (usize, usize) {
    let at = source.find(needle).expect("needle in the document");
    let line = source[..at].matches('\n').count();
    let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
    (line, source[line_start..at].chars().count() + 1)
}

#[test]
fn hover_shows_values_deprecation_and_docs_links() {
    let bundle = help();
    let mut failures = Vec::new();
    for (language, source) in DOCUMENTS {
        let behavior = language.behavior();
        let (line, column) = position_of(source, "fast");
        match key_hover(source, line, column, Some(&bundle), behavior) {
            Some(hover) => {
                let value = hover.value.as_ref();
                if hover.key != "mode"
                    || value.map(|value| value.label.as_str()) != Some("fast")
                    || value.and_then(|value| value.doc.description.as_deref())
                        != Some("Skips the checks.")
                    || hover.docs_url.as_deref() != Some("https://example.com/docs/mode")
                {
                    failures.push(format!(
                        "{language:?} value: key {} value {value:?} docs {:?}",
                        hover.key, hover.docs_url
                    ));
                }
            }
            None => failures.push(format!("{language:?}: no hover over the value")),
        }
        let (line, column) = position_of(source, "timeout");
        match key_hover(source, line, column, Some(&bundle), behavior) {
            Some(hover)
                if hover.key == "timeout"
                    && hover.deprecated
                    && hover.value.is_none()
                    && hover.deprecation.as_deref() == Some("Use deadline instead.") => {}
            Some(hover) => failures.push(format!(
                "{language:?} key: {} deprecated {} ({:?})",
                hover.key, hover.deprecated, hover.deprecation
            )),
            None => failures.push(format!("{language:?}: no hover over timeout")),
        }
        let (line, column) = position_of(source, "mode");
        let docs = key_hover(source, line, column, Some(&bundle), behavior)
            .and_then(|hover| hover.docs_url);
        if docs.as_deref() != Some("https://example.com/docs/mode") {
            failures.push(format!("{language:?}: the mode key links {docs:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Works with and without the `schema-validation` feature.
#[test]
fn deprecated_keys_in_the_document_get_a_warning_on_the_key() {
    let bundle = help();
    let mut failures = Vec::new();
    for (language, source) in DOCUMENTS {
        let diagnostics = validate(source, Some(&bundle), None, true, language.behavior());
        let (line, column) = position_of(source, "timeout");
        let expected_column = column + usize::from(language == Language::JSON);
        // The key only, as VS Code strikes it: never its value.
        let expected_key = if language == Language::JSON {
            "\"timeout\""
        } else {
            "timeout"
        };
        match diagnostics.as_slice() {
            [warning]
                if warning.severity == Severity::Warning
                    && warning.deprecated
                    && warning.end_line.is_none()
                    && warning.line == Some(line + 1)
                    && warning.column.is_some_and(|start| start <= expected_column)
                    && marked(source, warning).as_deref() == Some(expected_key)
                    && warning.message
                        == "/timeout: 'timeout' is deprecated. Use deadline instead." => {}
            other => failures.push(format!(
                "{language:?}: {other:?} marks {:?}",
                other.first().and_then(|warning| marked(source, warning))
            )),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Accepting `server` writes its required children: `host` takes the caret,
/// `port` its example, `scheme` its default; Tab selects those two in turn.
#[test]
fn required_children_take_examples_and_tab_selects_each_written_value() {
    let mut failures = Vec::new();
    for (language, marked) in ROOT {
        let options = offered(language, marked);
        let Some(option) = options.iter().find(|option| option.label == "server") else {
            failures.push(format!("{language:?}: server not offered"));
            continue;
        };
        let source = marked.replacen('|', "", 1);
        let (updated, _, caret) = apply_completion(&source, option);
        let insert_start = caret - option.cursor_offset;
        let stops: Vec<(usize, usize)> =
            stop_ranges(&option.insert, option.cursor_offset, &option.fills)
                .into_iter()
                .map(|(start, end)| (insert_start + start, insert_start + end))
                .collect();
        let region_end = insert_start + option.insert.encode_utf16().count();
        let units: Vec<u16> = updated.encode_utf16().collect();
        let mut walked = Vec::new();
        let mut at = caret;
        while let Some((anchor, end)) =
            next_stop(&updated, at, region_end, &stops, language.behavior())
        {
            walked.push(String::from_utf16_lossy(&units[anchor..end]));
            // Past the value, and its closing quote, as Tab steps.
            at = end + usize::from(units.get(end) == Some(&u16::from(b'"')));
            if walked.len() > 4 {
                break;
            }
        }
        if walked != ["8080", "https"] {
            failures.push(format!("{language:?}: {updated:?} walks {walked:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// An example fills only children the caret does not go to, and never
/// decides a key's own insert: `bind` (a number or a string, with a string
/// example) is not written as an empty string, and `metrics`, whose one
/// required child is `bind`, leaves it empty for the caret.
#[test]
fn examples_never_take_the_caret_or_shape_a_keys_own_value() {
    let mut failures = Vec::new();
    for (language, marked) in ROOT {
        let options = offered(language, marked);
        let source = marked.replacen('|', "", 1);
        for label in ["bind", "metrics"] {
            let Some(option) = options.iter().find(|option| option.label == label) else {
                failures.push(format!("{language:?}: {label} not offered"));
                continue;
            };
            let (updated, _, _) = apply_completion(&source, option);
            if updated.contains("0.0.0.0:7835") || updated.contains("\"\"") {
                failures.push(format!("{language:?} {label}: {updated:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn keys_matching_a_supported_pattern_get_docs_and_unsupported_ones_are_reported() {
    let bundle = help();
    assert_eq!(bundle.warnings.len(), 1, "{:?}", bundle.warnings);
    assert!(
        bundle.warnings[0].starts_with("/properties/labels/patternProperties: the pattern"),
        "{:?}",
        bundle.warnings
    );
    let documents: [(Language, &str); 5] = [
        (Language::YAML, "env:\n  PATH: x\n"),
        (Language::JSON, "{\"env\": {\"PATH\": \"x\"}}\n"),
        (Language::TOML, "[env]\nPATH = \"x\"\n"),
        (Language::HCL, "env {\n  PATH = \"x\"\n}\n"),
        (Language::XML, "<env>\n  <PATH>x</PATH>\n</env>\n"),
    ];
    let mut failures = Vec::new();
    for (language, source) in documents {
        let (line, column) = position_of(source, "PATH");
        let description = key_hover(source, line, column, Some(&bundle), language.behavior())
            .and_then(|hover| hover.description);
        if description.as_deref() != Some("One variable.") {
            failures.push(format!("{language:?}: {description:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A key's hover card shows the property's `title` above its description
/// (as VS Code's JSON hover does), in every language; a property without
/// one shows none.
#[test]
fn hover_shows_a_keys_title() {
    let bundle = build_schema(
        r#"{"properties": {
            "listen": {"type": "integer", "title": "Listen port", "description": "Where it listens."},
            "name": {"type": "string", "description": "No title."}
        }}"#,
    )
    .unwrap();
    let documents = [
        (Language::YAML, "listen: 1\nname: a\n"),
        (
            Language::JSON,
            "{\n  \"listen\": 1,\n  \"name\": \"a\"\n}\n",
        ),
        (Language::TOML, "listen = 1\nname = \"a\"\n"),
        (Language::HCL, "listen = 1\nname = \"a\"\n"),
        (Language::XML, "<listen>1</listen>\n<name>a</name>\n"),
    ];
    let mut failures = Vec::new();
    for (language, source) in documents {
        let (line, column) = position_of(source, "listen");
        let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
        let shown = hover
            .as_ref()
            .map(|hover| (hover.title.as_deref(), hover.description.as_deref()));
        if shown != Some((Some("Listen port"), Some("Where it listens."))) {
            failures.push(format!("{language:?} listen: {shown:?}"));
        }
        let (line, column) = position_of(source, "name");
        let title = key_hover(source, line, column, Some(&bundle), language.behavior())
            .map(|hover| hover.title);
        if title != Some(None) {
            failures.push(format!("{language:?} name: {title:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
