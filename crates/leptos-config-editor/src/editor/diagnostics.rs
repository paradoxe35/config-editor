use super::language::LanguageBehavior;
#[cfg(feature = "schema-validation")]
use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

#[cfg(feature = "schema-validation")]
const MAX_DIAGNOSTICS: usize = 20;

/// Delay after the last keystroke before revalidating, in milliseconds. Long
/// enough that intermediate states while typing (an unclosed bracket, a half
/// typed key) do not flash squiggles on every keystroke.
pub(super) const DIAGNOSTICS_DEBOUNCE_MS: i32 = 250;

pub(super) struct SchemaBundle {
    pub(super) schema: Value,
    #[cfg(feature = "schema-validation")]
    validator: jsonschema::Validator,
}

pub(super) fn build_schema(schema_text: &str) -> Result<SchemaBundle, String> {
    let schema: Value = serde_json::from_str(schema_text)
        .map_err(|error| format!("Invalid JSON Schema: {error}"))?;
    #[cfg(feature = "schema-validation")]
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| format!("Invalid JSON Schema: {error}"))?;
    Ok(SchemaBundle {
        schema,
        #[cfg(feature = "schema-validation")]
        validator,
    })
}

#[derive(Clone, PartialEq)]
pub(super) struct Diagnostic {
    pub(super) line: Option<usize>,
    pub(super) column: Option<usize>,
    pub(super) end_column: Option<usize>,
    pub(super) message: String,
}
/// Shared validation orchestration: parses through the language behavior,
/// maps schema violations to source spans through it, and returns inline
/// diagnostics.
pub(super) fn validate(
    source: &str,
    schema: Option<&SchemaBundle>,
    schema_error: Option<&str>,
    enabled: bool,
    behavior: &dyn LanguageBehavior,
) -> Vec<Diagnostic> {
    if !enabled {
        return Vec::new();
    }
    if source.trim().is_empty() {
        return Vec::new();
    }

    let document: Value = match behavior.parse(source) {
        Ok(document) => document,
        Err(error) => {
            return vec![Diagnostic {
                line: error.line,
                column: error.column,
                end_column: error.column.map(|column| column + 1),
                message: error.message,
            }];
        }
    };

    if let Some(error) = schema_error {
        return vec![Diagnostic {
            line: None,
            column: None,
            end_column: None,
            message: error.to_string(),
        }];
    }

    #[cfg(feature = "schema-validation")]
    if let Some(schema) = schema {
        return schema
            .validator
            .iter_errors(&document)
            .take(MAX_DIAGNOSTICS)
            .map(|error| {
                let (path, message) = most_specific_validation_error(&error);
                let location = if let ValidationErrorKind::AdditionalProperties { unexpected } =
                    error.kind()
                {
                    let parent = error.instance_path().as_str();
                    unexpected.iter().find_map(|key| {
                        behavior.span_for_path(source, &format!("{parent}/{}", escape_pointer(key)))
                    })
                } else {
                    behavior
                        .value_span_for_path(source, &path)
                        .or_else(|| behavior.span_for_path(source, &path))
                }
                .or_else(|| behavior.first_span(source));
                Diagnostic {
                    line: location.map(|location| location.0),
                    column: location.map(|location| location.1),
                    end_column: location.map(|location| location.2),
                    message: if path.is_empty() {
                        message
                    } else {
                        format!("{path}: {message}")
                    },
                }
            })
            .collect();
    }
    let _ = schema;
    Vec::new()
}

#[cfg(feature = "schema-validation")]
fn most_specific_validation_error(error: &ValidationError<'_>) -> (String, String) {
    let path = error.instance_path().as_str().to_string();
    let nested = match error.kind() {
        ValidationErrorKind::AnyOf { context }
        | ValidationErrorKind::OneOfNotValid { context }
        | ValidationErrorKind::OneOfMultipleValid { context } => context,
        _ => return (path, error.to_string()),
    };
    let nested_error = nested
        .iter()
        .flatten()
        .map(most_specific_validation_error)
        .max_by_key(|(nested_path, _)| nested_path.matches('/').count());
    match nested_error {
        Some((nested_path, message))
            if nested_path.matches('/').count() > path.matches('/').count() =>
        {
            (nested_path, message)
        }
        _ => (path, error.to_string()),
    }
}

fn escape_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}
