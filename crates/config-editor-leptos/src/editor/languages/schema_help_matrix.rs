//! Richer schema help, checked in every language against the schema-help
//! fixture: deprecated keys, docs links, per-value descriptions, required
//! children filled from examples with Tab stops, and patterns the regex
//! engine cannot read. Each test lists every failing case at once.
//!
//! `|` marks the caret.

use super::super::completion::{
    Completion, apply_completion, completions, key_hover, next_stop, stop_ranges,
};
use super::super::diagnostics::{Diagnostic, Severity, validate};
use super::super::schema::{SchemaBundle, build_schema};
use super::Language;
use super::harness::{
    At, completing, every_language, hovering, key_place, key_text, report, text, value_place, write,
};
use crate::editor::text::{TextRange, TextSize};
use serde_json::{Value, json};

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

/// Where a new top-level key is typed, in each language.
fn root() -> Vec<(Language, String)> {
    let at = At::NewKey {
        object: "",
        typed: "",
    };
    completing()
        .into_iter()
        .map(|language| {
            let marked = write(language, &json!({}), &[], at)
                .map_or_else(|error| error, |written| written.marked());
            (language, marked)
        })
        .collect()
}

/// `document` as `language` writes it, or why it cannot.
fn written(language: Language, document: &Value) -> String {
    text(language, document, &[]).unwrap_or_else(|error| error)
}

/// The zero-based line and one-based column of the key at `pointer`.
fn key_at(language: Language, source: &str, pointer: &str) -> (usize, usize) {
    key_place(language, source, pointer, 0).map_or((0, 0), |(line, column)| (line, column + 1))
}

#[test]
fn deprecated_keys_are_marked_and_listed_after_the_others() {
    let mut failures = Vec::new();
    for (language, marked) in root() {
        let options = offered(language, &marked);
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
    report(&failures);
}

#[test]
fn values_show_their_own_title_and_description() {
    let cases = [
        ("mode", "fast", "Skips the checks."),
        ("mode", "safe", "Checks everything."),
        ("mode", "debug", "Logs every step."),
        ("level", "trace", "Trace — Everything, very verbose."),
        ("level", "info", "Info — Normal messages."),
    ];
    let mut failures = Vec::new();
    for language in completing() {
        for (key, label, detail) in cases {
            let pointer = format!("/{key}");
            let marked = write(language, &json!({ key: "" }), &[], At::EmptyValue(&pointer))
                .map_or_else(|error| error, |written| written.marked());
            let options = offered(language, &marked);
            let found = options.iter().find(|option| option.label == label);
            if found.map(|option| option.detail.as_str()) != Some(detail) {
                let all: Vec<(&str, &str)> = options
                    .iter()
                    .map(|option| (option.label.as_str(), option.detail.as_str()))
                    .collect();
                failures.push(format!("{language:?} {marked:?} {label}: {all:?}"));
            }
        }
    }
    report(&failures);
}

/// A document with `mode: fast` and `timeout: 3`.
fn mode_and_timeout() -> Value {
    json!({"mode": "fast", "timeout": 3})
}

/// The text a diagnostic underlines (on its one line).
fn marked(source: &str, diagnostic: &Diagnostic) -> Option<String> {
    let line = source.split('\n').nth(diagnostic.line? - 1)?;
    let column = diagnostic.column?;
    let end = diagnostic.end_column?;
    Some(line.chars().skip(column - 1).take(end - column).collect())
}

#[test]
fn hover_shows_values_deprecation_and_docs_links() {
    let bundle = help();
    let mut failures = Vec::new();
    for language in hovering() {
        let source = &written(language, &mode_and_timeout());
        let behavior = language.behavior();
        let (line, column) = value_place(language, source, "/mode", "fast", 0)
            .map_or((0, 0), |(line, column)| (line, column + 1));
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
        let (line, column) = key_at(language, source, "/timeout");
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
        let (line, column) = key_at(language, source, "/mode");
        let docs = key_hover(source, line, column, Some(&bundle), behavior)
            .and_then(|hover| hover.docs_url);
        if docs.as_deref() != Some("https://example.com/docs/mode") {
            failures.push(format!("{language:?}: the mode key links {docs:?}"));
        }
    }
    report(&failures);
}

/// Works with and without the `schema-validation` feature.
#[test]
fn deprecated_keys_in_the_document_get_a_warning_on_the_key() {
    let bundle = help();
    let mut failures = Vec::new();
    for language in every_language() {
        let source = &written(language, &mode_and_timeout());
        let diagnostics = validate(source, Some(&bundle), None, true, language.behavior());
        let (line, column) = key_at(language, source, "/timeout");
        // The key only, as VS Code strikes it (with its quotes): never its
        // value.
        let expected_key = key_text(language, source, "/timeout").unwrap_or_default();
        let expected_column = column + usize::from(expected_key.len() > "timeout".len());
        let expected_key = expected_key.as_str();
        match diagnostics.as_slice() {
            [warning]
                if warning.severity == Severity::Warning
                    && warning.deprecated
                    && warning.end_line.is_none()
                    && warning.line == Some(line + 1)
                    && warning.column.is_some_and(|start| start <= expected_column)
                    && marked(source, warning).as_deref() == Some(expected_key)
                    && warning.message
                        == r#"/timeout: "timeout" is deprecated. Use deadline instead."# => {}
            other => failures.push(format!(
                "{language:?}: {other:?} marks {:?}",
                other.first().and_then(|warning| marked(source, warning))
            )),
        }
    }
    report(&failures);
}

/// Accepting `server` writes its required children: `host` takes the caret,
/// `port` its example, `scheme` its default; Tab selects those two in turn.
#[test]
fn required_children_take_examples_and_tab_selects_each_written_value() {
    let mut failures = Vec::new();
    for (language, marked) in root() {
        let options = offered(language, &marked);
        let Some(option) = options.iter().find(|option| option.label == "server") else {
            failures.push(format!("{language:?}: server not offered"));
            continue;
        };
        let source = marked.replacen('|', "", 1);
        let (updated, _, caret) = apply_completion(&source, option);
        let insert_start = caret - option.cursor_offset;
        let stops: Vec<TextRange> =
            stop_ranges(&option.insert, option.cursor_offset, &option.fills)
                .into_iter()
                .map(|stop| stop + insert_start)
                .collect();
        let region_end = insert_start + TextSize::of(option.insert.as_str());
        let mut walked = Vec::new();
        let mut at = caret;
        while let Some((anchor, end)) =
            next_stop(&updated, at, region_end, &stops, language.behavior())
        {
            walked.push(updated[TextRange::new(anchor, end)].to_string());
            // Past the value, and its closing quote, as Tab steps.
            at = end
                + if updated[usize::from(end)..].starts_with('"') {
                    TextSize::of('"')
                } else {
                    TextSize::default()
                };
            if walked.len() > 4 {
                break;
            }
        }
        if walked != ["8080", "https"] {
            failures.push(format!("{language:?}: {updated:?} walks {walked:?}"));
        }
    }
    report(&failures);
}

/// An example fills only children the caret does not go to, and never
/// decides a key's own insert: `bind` (a number or a string, with a string
/// example) is not written as an empty string, and `metrics`, whose one
/// required child is `bind`, leaves it empty for the caret.
#[test]
fn examples_never_take_the_caret_or_shape_a_keys_own_value() {
    let mut failures = Vec::new();
    for (language, marked) in root() {
        let options = offered(language, &marked);
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
    report(&failures);
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
    let mut failures = Vec::new();
    for language in hovering() {
        let source = &written(language, &json!({"env": {"PATH": "x"}}));
        let (line, column) = key_at(language, source, "/env/PATH");
        let description = key_hover(source, line, column, Some(&bundle), language.behavior())
            .and_then(|hover| hover.description);
        if description.as_deref() != Some("One variable.") {
            failures.push(format!("{language:?}: {description:?}"));
        }
    }
    report(&failures);
}

/// A key's hover card shows the property's `title` above its description
/// (as VS Code's JSON hover does), in every language; a property without
/// one shows none.
#[test]
fn hover_shows_a_keys_title() {
    let bundle = build_schema(
        r#"{"properties": {
            "listen": {"type": "integer", "title": "Listen port", "description": "Where it listens."},
            "name": {"type": "string", "description": "No title."},
            "level": {"oneOf": [
                {"const": "info", "title": "Info", "description": "Normal messages."},
                {"const": "trace", "title": "Trace"}
            ]}
        }}"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for language in hovering() {
        let source = &written(
            language,
            &json!({"listen": 1, "name": "a", "level": "trace"}),
        );
        let (line, column) = key_at(language, source, "/listen");
        let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
        let shown = hover
            .as_ref()
            .map(|hover| (hover.title.as_deref(), hover.description.as_deref()));
        if shown != Some((Some("Listen port"), Some("Where it listens."))) {
            failures.push(format!("{language:?} listen: {shown:?}"));
        }
        let (line, column) = key_at(language, source, "/name");
        let title = key_hover(source, line, column, Some(&bundle), language.behavior())
            .map(|hover| hover.title);
        if title != Some(None) {
            failures.push(format!("{language:?} name: {title:?}"));
        }
        // A `oneOf` of values gives the values titles and descriptions,
        // not the key: the first value's are not the key's own.
        let (line, column) = key_at(language, source, "/level");
        let shown = key_hover(source, line, column, Some(&bundle), language.behavior())
            .map(|hover| (hover.title, hover.description));
        if shown != Some((None, None)) {
            failures.push(format!("{language:?} level: {shown:?}"));
        }
    }
    report(&failures);
}
