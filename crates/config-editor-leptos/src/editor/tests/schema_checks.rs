//! A schema format's extra checks (`SchemaCheck`) run where the editor
//! validates, through the same door for any format: a tiny format here
//! (keys in alphabetical order), no XSD. Their problems are placed by
//! pointer like the schema's, worded with the pointer first, capped with
//! them, and each check runs once per validation.

use std::borrow::Cow;
use std::sync::Arc;

use serde_json::json;

use crate::editor::diagnostics::{MAX_DIAGNOSTICS, Schema};
use crate::editor::work_count::{Work, counted};
use crate::editor::{
    CheckProblem, CheckedDocument, CompiledSchema, Language, SchemaCheck, SchemaCompiler,
    SchemaError, SchemaFormat, SchemaFormatInfo, SchemaInput, Severity,
};

/// Each key of an object after one it sorts before: out of order. Its
/// message names the language of the text it was given.
struct Sorted;

impl SchemaCheck for Sorted {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("keys in order")
    }

    fn check(&self, document: &CheckedDocument<'_>) -> Vec<CheckProblem> {
        let mut problems = Vec::new();
        let mut stack = vec![(String::new(), document.value)];
        while let Some((pointer, value)) = stack.pop() {
            let Some(map) = value.as_object() else {
                continue;
            };
            let keys: Vec<&String> = map.keys().collect();
            for pair in keys.windows(2) {
                if pair[0] > pair[1] {
                    let label = document.language.info().label;
                    problems.push(
                        CheckProblem::new(
                            format!("{pointer}/{}", pair[1]),
                            format!("comes after {} ({label})", pair[0]),
                        )
                        .at_key()
                        .severity(Severity::Warning),
                    );
                }
            }
            for (key, child) in map {
                stack.push((format!("{pointer}/{key}"), child));
            }
        }
        if document.text.contains("everything") {
            problems.push(CheckProblem::new("", "the whole document"));
        }
        problems
    }
}

/// Any schema text: an open JSON Schema with the check.
struct SortedFormat;

impl SchemaCompiler for SortedFormat {
    fn info(&self) -> SchemaFormatInfo {
        SchemaFormatInfo {
            name: "sorted",
            label: "Sorted",
            extensions: &["sorted"],
        }
    }

    fn compile(&self, _input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        Ok(CompiledSchema::new(json!({"type": "object"})).with_checks(vec![Arc::new(Sorted)]))
    }
}

static SORTED: SortedFormat = SortedFormat;

fn schema() -> Schema {
    Schema::compile_as(SchemaFormat::new(&SORTED), &SchemaInput::new("")).unwrap()
}

#[test]
fn a_check_of_any_format_is_shown_with_the_schemas_problems_at_its_key() {
    let schema = schema();
    let problems = schema.validate(Language::YAML, "b: 1\na:\n  y: 2\n  x: 3\n");
    let found: Vec<(String, Option<usize>, Severity)> = problems
        .into_iter()
        .map(|problem| (problem.message, problem.line, problem.severity))
        .collect();
    assert_eq!(
        found,
        [
            (
                "/a: comes after b (YAML)".to_string(),
                Some(2),
                Severity::Warning
            ),
            (
                "/a/x: comes after y (YAML)".to_string(),
                Some(4),
                Severity::Warning
            ),
        ]
    );
    // At the root: no pointer before the message.
    let problems = schema.validate(Language::YAML, "a: everything\n");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].message, "the whole document");
    assert_eq!(problems[0].severity, Severity::Error);
}

#[cfg(feature = "json")]
#[test]
fn a_check_is_given_the_text_in_its_language() {
    let problems = schema().validate(Language::JSON, "{\"b\": 1,\n \"a\": 2}\n");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].message, "/a: comes after b (JSON)");
    assert_eq!(problems[0].line, Some(2));
}

#[test]
fn a_check_runs_once_per_validation_and_its_problems_are_capped_with_the_rest() {
    let schema = schema();
    let mut text = String::new();
    for index in (0..100).rev() {
        text.push_str(&format!("k{index:03}: 1\n"));
    }
    let (problems, checks) = counted(Work::SchemaCheck, || schema.validate(Language::YAML, &text));
    assert_eq!(checks, 1);
    assert_eq!(problems.len(), MAX_DIAGNOSTICS + 1, "the cap and its note");
    // No check, no work.
    let plain = Schema::compile(r#"{"type": "object"}"#).unwrap();
    let (_, checks) = counted(Work::SchemaCheck, || plain.validate(Language::YAML, &text));
    assert_eq!(checks, 0);
}
