//! A leading byte order mark is no part of the document: every place in
//! the text is found after it, and a save from another view keeps it.

use super::Language;
use super::diagnostics::Severity;
use super::host_diagnostics::{HostDiagnostic, RangeText, place};
use super::paths::{field_range, path_at};
use super::save_matrix::{ALL, report, save_value, slot};
use crate::strings::default_strings;
use serde_json::{Value, json};

const BOM: char = '\u{FEFF}';

/// One small document per language, its first key commented.
fn sample(language: Language) -> &'static str {
    [
        (Language::YAML, "a: 1 # c\nb:\n  c: 2\n"),
        (
            Language::JSON,
            "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": 2\n  }\n}\n",
        ),
        (Language::TOML, "a = 1 # c\n\n[b]\nc = 2\n"),
        (Language::HCL, "a = 1 # c\n\nb {\n  c = 2\n}\n"),
    ]
    .into_iter()
    .find(|(known, _)| *known == language)
    .map_or(
        "<r>\n  <a>1</a> <!-- c -->\n  <b>\n    <c>2</c>\n  </b>\n</r>\n",
        |(_, text)| text,
    )
}

fn pointers(language: Language) -> Vec<&'static str> {
    if language == Language::XML {
        vec!["/r", "/r/a", "/r/b", "/r/b/c"]
    } else {
        vec!["/a", "/b", "/b/c"]
    }
}

fn first_key(language: Language) -> &'static str {
    if language == Language::XML {
        "/r/a"
    } else {
        "/a"
    }
}

/// `(line, start, end)` moved one column on when on the first line.
fn shifted(line: usize, start: usize, end: usize) -> (usize, usize, usize) {
    let shift = usize::from(line == 1);
    (line, start + shift, end + shift)
}

/// Every path is found in the marked text where it is in the unmarked
/// one, one column on in the first line.
#[test]
fn every_path_is_found_after_a_byte_order_mark() {
    let mut failures = Vec::new();
    for language in ALL {
        let behavior = language.behavior();
        let plain = sample(language);
        let marked = format!("{BOM}{plain}");
        for pointer in pointers(language) {
            let range = |text: &str| {
                field_range(behavior, text, pointer).map(|range| {
                    (
                        range.start.line,
                        range.start.column,
                        range.end.line,
                        range.end.column,
                    )
                })
            };
            let expected = range(plain).map(|(line, start, end_line, end)| {
                (
                    line,
                    start + usize::from(line == 1),
                    end_line,
                    end + usize::from(end_line == 1),
                )
            });
            let found = range(&marked);
            if expected.is_none() || found != expected {
                failures.push(format!(
                    "{language} range {pointer}: {found:?}, expected {expected:?}"
                ));
            }
            let spans = [
                (
                    "key",
                    behavior.key_span(plain, pointer),
                    behavior.key_span(&marked, pointer),
                ),
                (
                    "value",
                    behavior.value_span(plain, pointer),
                    behavior.value_span(&marked, pointer),
                ),
            ];
            for (what, plain_span, marked_span) in spans {
                let expected =
                    plain_span.map(|span| shifted(span.line, span.start_column, span.end_column));
                let found = marked_span.map(|span| (span.line, span.start_column, span.end_column));
                if found != expected {
                    failures.push(format!(
                        "{language} {what} {pointer}: {found:?}, expected {expected:?}"
                    ));
                }
            }
            let batched = |text: &str| {
                let wanted = [pointer.to_string()];
                (
                    behavior.key_spans(text, &wanted)[0],
                    behavior.value_spans(text, &wanted)[0],
                )
            };
            let (keys, values) = batched(&marked);
            let (plain_keys, plain_values) = batched(plain);
            let moved = |span: Option<super::language::Span>| {
                span.map(|span| shifted(span.line, span.start_column, span.end_column))
            };
            let pair = |span: Option<super::language::Span>| {
                span.map(|span| (span.line, span.start_column, span.end_column))
            };
            if pair(keys) != moved(plain_keys) || pair(values) != moved(plain_values) {
                failures.push(format!("{language} batched {pointer}: {keys:?} {values:?}"));
            }
        }
    }
    report(failures);
}

/// The caret on every character of the first line is on the path it is
/// on without the mark.
#[test]
fn the_path_at_the_caret_is_read_after_a_byte_order_mark() {
    let mut failures = Vec::new();
    for language in ALL {
        let behavior = language.behavior();
        let plain = sample(language);
        let marked = format!("{BOM}{plain}");
        for cursor in 0..plain.encode_utf16().count() {
            let expected = path_at(behavior, plain, cursor).map(|path| path.to_string());
            let found = path_at(behavior, &marked, cursor + 1).map(|path| path.to_string());
            if found != expected {
                failures.push(format!(
                    "{language} at {cursor}: {found:?}, expected {expected:?}"
                ));
            }
        }
    }
    report(failures);
}

/// A host's problem on each field is placed as without the mark.
#[test]
fn problems_are_placed_after_a_byte_order_mark() {
    let mut failures = Vec::new();
    for language in ALL {
        let plain = sample(language);
        let marked = format!("{BOM}{plain}");
        let problems: Vec<HostDiagnostic> = pointers(language)
            .into_iter()
            .map(|pointer| HostDiagnostic::at_path(pointer, Severity::Error, pointer))
            .collect();
        let placed = |text: &str| {
            place(
                language.behavior(),
                text,
                &problems,
                RangeText::OnScreen,
                &[],
                default_strings(),
            )
            .into_iter()
            .map(|problem| {
                (
                    problem.message,
                    problem.line,
                    problem.column,
                    problem.end_column,
                )
            })
            .collect::<Vec<_>>()
        };
        let expected: Vec<_> = placed(plain)
            .into_iter()
            .map(|(message, line, column, end)| {
                let shift = usize::from(line == Some(1));
                (
                    message,
                    line,
                    column.map(|c| c + shift),
                    end.map(|c| c + shift),
                )
            })
            .collect();
        let found = placed(&marked);
        if found != expected || expected.iter().any(|(_, line, _, _)| line.is_none()) {
            failures.push(format!("{language}: {found:?}\nexpected {expected:?}"));
        }
    }
    report(failures);
}

type Change = fn(&mut Value, &str);

fn edit(value: &mut Value, pointer: &str) {
    *slot(value, pointer) = json!(10);
}

fn parent<'a>(value: &'a mut Value, pointer: &str) -> &'a mut serde_json::Map<String, Value> {
    let (parent, _) = pointer.rsplit_once('/').unwrap();
    let parent = if parent.is_empty() {
        value
    } else {
        slot(value, parent)
    };
    parent.as_object_mut().unwrap()
}

fn remove(value: &mut Value, pointer: &str) {
    let key = pointer.rsplit('/').next().unwrap();
    parent(value, pointer).shift_remove(key);
}

fn rename(value: &mut Value, pointer: &str) {
    let key = pointer.rsplit('/').next().unwrap();
    let object = parent(value, pointer);
    *object = std::mem::take(object)
        .into_iter()
        .map(|(name, held)| (if name == key { "z".to_string() } else { name }, held))
        .collect();
}

fn add_before(value: &mut Value, pointer: &str) {
    parent(value, pointer).shift_insert(0, "n".to_string(), json!(5));
}

/// The first key edited, renamed, removed, or a key added before it, from
/// every other view, saves as it does without the mark, the mark kept.
#[test]
fn a_save_from_another_view_keeps_a_byte_order_mark_and_the_comments() {
    let mut failures = Vec::new();
    let changes: [(&str, Change); 4] = [
        ("edit", edit),
        ("remove", remove),
        ("rename", rename),
        ("add", add_before),
    ];
    for host in ALL {
        let plain = sample(host);
        let marked = format!("{BOM}{plain}");
        for (label, change) in changes {
            let change = |value: &mut Value| change(value, first_key(host));
            for view in ALL.into_iter().filter(|view| *view != host) {
                let expected =
                    save_value(host, view, plain, &change).map(|saved| format!("{BOM}{saved}"));
                let saved = save_value(host, view, &marked, &change);
                if expected.is_err() || saved != expected {
                    failures.push(format!(
                        "{label} {host} via {view}:\n--- expected\n{expected:?}\n--- saved\n{saved:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}
