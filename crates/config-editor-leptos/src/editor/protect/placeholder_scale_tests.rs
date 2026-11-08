//! Placeholder problems on documents with thousands of placeholders, in
//! every language: the same verdicts as a path lookup for each token, in
//! one parse (not one per token), and the real placeholders of a text that
//! does not parse (mid-typing) kept real without a lookup each.

use serde_json::{Value, json};

use super::super::Language;
use super::super::language::Repetition;
use super::super::paths::path_at;
use super::super::secret_options::SecretOptions;
use super::marks::marks_at;
use super::problems::unknown_placeholders;
use super::texts;
use super::tokens::{SecretMatcher, locate_written};

/// `clients` placeholders in secret fields (one in a list) and in plain
/// names, each token once.
fn document(clients: usize) -> Value {
    let mut map = serde_json::Map::new();
    for index in 0..clients {
        map.insert(
            format!("c{index}"),
            json!({
                "name": format!("client {index} «secret {index:x}b»"),
                "secret": format!("«secret {index:x}a»"),
                "keys": [format!("«secret {index:x}c»")],
            }),
        );
    }
    json!({ "listen": 7835, "clients": map })
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "clients": {
                "additionalProperties": {
                    "properties": {
                        "name": {"type": "string"},
                        "secret": {"type": "string", "x-secret": true},
                        "keys": {"type": "array", "items": {"type": "string", "x-secret": true}}
                    }
                }
            }
        }
    })
}

fn languages() -> [Language; 5] {
    [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ]
}

fn written(language: Language, clients: usize) -> String {
    language
        .behavior()
        .serialize(&document(clients), None)
        .expect("the document writes in every language")
}

/// The problems found by looking up each token's path: what the one-parse
/// reading must agree with.
fn by_path(language: Language, text: &str, schema: &Value) -> Vec<(usize, usize, String)> {
    let behavior = language.behavior();
    let repeats = behavior.repetition() != Repetition::None;
    let strings = crate::strings::default_strings();
    SecretMatcher::new(&SecretOptions::default())
        .find(text)
        .into_iter()
        .filter_map(|range| {
            let path = path_at(behavior, text, text[..range.start].encode_utf16().count())?;
            marks_at(schema, path.segments(), repeats).1.then(|| {
                let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
                (
                    text[..line_start].matches('\n').count() + 1,
                    text[line_start..range.start].chars().count() + 1,
                    texts::unknown_placeholder(strings, &path.to_pointer()),
                )
            })
        })
        .collect()
}

fn found(language: Language, text: &str, schema: &Value) -> Vec<(usize, usize, String)> {
    unknown_placeholders(
        language.behavior(),
        text,
        &SecretMatcher::new(&SecretOptions::default()),
        &[],
        &[],
        Some(schema),
        crate::strings::default_strings(),
    )
    .into_iter()
    .map(|problem| {
        (
            problem.line.unwrap_or_default(),
            problem.column.unwrap_or_default(),
            problem.message,
        )
    })
    .collect()
}

#[test]
fn placeholders_read_in_one_parse_agree_with_a_path_lookup_each_in_every_language() {
    let schema = schema();
    let mut failures = Vec::new();
    for language in languages() {
        let text = written(language, 6);
        // One token written in two fields: the parse cannot say which
        // occurrence is where, so those are looked up by path.
        let twice = text.replacen("«secret 3b»", "«secret 2a»", 1);
        // Pasted again: where that parses (repeated elements), every token
        // twice, each copy in its own field.
        let pasted = format!("{text}\n{text}");
        let mut cases = vec![("as written", text), ("a token twice", twice)];
        if language.behavior().parse(&pasted).is_ok() {
            cases.push(("pasted again", pasted));
        }
        for (case, text) in cases {
            let expected = by_path(language, &text, &schema);
            let flagged = found(language, &text, &schema);
            if expected.len() < 12 || flagged != expected {
                failures.push(format!(
                    "{language} {case}:\n  by path {expected:?}\n  found   {flagged:?}\n{text}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn thousands_of_placeholders_are_checked_in_one_parse_in_every_language() {
    let schema = schema();
    let mut failures = Vec::new();
    for language in languages() {
        let text = written(language, 2_000);
        let (took, flagged) =
            crate::editor::test_timing::best_of(3, || found(language, &text, &schema));
        // A path lookup for each token reads the whole document 6,000 times.
        if flagged.len() != 4_000 || took.as_millis() > 400 {
            failures.push(format!(
                "{language}: {} flagged in {took:?} ({} lines)",
                flagged.len(),
                text.lines().count()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn real_placeholders_stay_real_while_a_large_text_does_not_parse_in_every_language() {
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let mut failures = Vec::new();
    for language in languages() {
        let behavior = language.behavior();
        let host = written(language, 2_000);
        let known = matcher.known(behavior, &host);
        // Half a line typed at the end.
        let typing = format!("{host}\n<{{\n");
        if behavior.parse(&typing).is_ok() {
            failures.push(format!("{language}: the typed text parses"));
            continue;
        }
        let followed = locate_written(&matcher, &typing, &known);
        let (took, flagged) = crate::editor::test_timing::best_of(3, || {
            unknown_placeholders(
                behavior,
                &typing,
                &matcher,
                &known,
                &followed,
                None,
                crate::strings::default_strings(),
            )
        });
        if !flagged.is_empty() || took.as_millis() > 400 {
            failures.push(format!(
                "{language}: {} flagged in {took:?}: {:?}",
                flagged.len(),
                flagged.first()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_large_document_pasted_again_is_checked_without_a_lookup_per_copy_in_every_language() {
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let mut failures = Vec::new();
    for language in languages() {
        let behavior = language.behavior();
        let host = written(language, 1_000);
        let known = matcher.known(behavior, &host);
        // Pasted at the end: each token written twice, the copies not real.
        let twice = format!("{host}\n{host}");
        let followed = locate_written(&matcher, &twice, &known);
        let (took, _) = crate::editor::test_timing::best_of(3, || {
            unknown_placeholders(
                behavior,
                &twice,
                &matcher,
                &known,
                &followed,
                None,
                crate::strings::default_strings(),
            )
        });
        if took.as_millis() > 400 {
            failures.push(format!("{language}: checked in {took:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The typing pause after a whole-document edit (Ctrl+A Tab, an undo)
/// finds the real placeholders again: one parse, not a path lookup each.
#[test]
fn thousands_of_placeholders_are_found_again_in_one_parse_in_every_language() {
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let mut failures = Vec::new();
    for language in languages() {
        let behavior = language.behavior();
        let host = written(language, 2_000);
        let known = matcher.known(behavior, &host);
        let (took, found) =
            crate::editor::test_timing::best_of(3, || matcher.locate(behavior, &host, &known));
        let expected = locate_written(&matcher, &host, &known);
        if found != expected || found.len() != 6_000 || took.as_millis() > 400 {
            failures.push(format!(
                "{language}: {} of {} found in {took:?}",
                found.len(),
                expected.len()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
