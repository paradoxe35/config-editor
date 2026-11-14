//! Running a schema's extra checks (`SchemaCheck`, from its format) where
//! the editor validates: on the same text and value, their problems placed
//! by pointer as the schema's own are, under the same cap. The core knows
//! nothing of what a check checks.

use serde_json::Value;

use super::diagnostics::{Diagnostic, SchemaBundle};
use super::language::ConfigLanguage;
use super::schema_formats::CheckedDocument;
use super::work_count::{Work, note};

/// The problems `schema`'s checks find in `document` (`source` read in
/// `behavior`), placed in `source`.
pub(super) fn problems(
    schema: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    source: &str,
    document: &Value,
) -> Vec<Diagnostic> {
    if schema.checks.is_empty() {
        return Vec::new();
    }
    let checked = CheckedDocument {
        text: source,
        language: behavior,
        value: document,
        schema: &schema.schema,
    };
    let found: Vec<_> = schema
        .checks
        .iter()
        .flat_map(|check| {
            note(Work::SchemaCheck);
            check.check(&checked)
        })
        .take(super::diagnostics::MAX_DIAGNOSTICS + 1)
        .collect();
    let pointers: Vec<String> = found
        .iter()
        .map(|problem| problem.pointer.clone())
        .collect();
    super::paths::with_spans(behavior, source, &pointers, || {
        found
            .into_iter()
            .map(|problem| {
                let written_as_block =
                    document
                        .pointer(&problem.pointer)
                        .is_some_and(|value| match value {
                            Value::Object(map) => !map.is_empty(),
                            Value::Array(items) => !items.is_empty(),
                            _ => false,
                        });
                let location = super::paths::problem_span(
                    behavior,
                    source,
                    &problem.pointer,
                    problem.at_key,
                    written_as_block,
                )
                .or_else(|| behavior.first_span(source));
                // As the schema's problems: the pointer first, but at the
                // root.
                let message = if problem.pointer.is_empty() {
                    problem.message
                } else {
                    format!("{}: {}", problem.pointer, problem.message)
                };
                Diagnostic {
                    line: location.map(|location| location.line),
                    column: location.map(|location| location.start_column),
                    end_column: location.map(|location| location.end_column),
                    message,
                    severity: problem.severity,
                    ..Diagnostic::default()
                }
            })
            .collect()
    })
}
