//! Running a schema's extra checks (`SchemaCheck`, from its format) where
//! the editor validates: on the same text and value, their problems placed
//! by pointer as the schema's own are, under the same cap. The core knows
//! nothing of what a check checks; a check's message may quote the
//! document's values, so what it quotes of a secret is hidden as in every
//! message the editor shows.

#![deny(clippy::string_slice)]

use serde_json::Value;

use super::data_reading::DataReading;
use super::diagnostics::Diagnostic;
use super::language::ConfigLanguage;
use super::schema::SchemaBundle;
use super::schema_formats::CheckedDocument;
use super::work_count::{Work, note};
use crate::EditorStrings;

/// The problems `schema`'s checks find in a document (`source` read in
/// `behavior` as `reading`), worded by `strings` and placed in `source`,
/// each message passed through `hide` (which hides the secrets it quotes).
pub(super) fn problems(
    schema: &SchemaBundle,
    (behavior, source, reading): (&dyn ConfigLanguage, &str, &DataReading),
    strings: &EditorStrings,
    hide: &dyn Fn(&str) -> String,
) -> Vec<Diagnostic> {
    if schema.checks.is_empty() {
        return Vec::new();
    }
    let document = reading.value();
    let checked = CheckedDocument {
        text: source,
        language: behavior,
        value: document,
        schema: &schema.schema,
        strings,
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
                let shown = hide(&problem.message);
                let message = if problem.pointer.is_empty() {
                    shown
                } else {
                    format!("{}: {shown}", problem.pointer)
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
