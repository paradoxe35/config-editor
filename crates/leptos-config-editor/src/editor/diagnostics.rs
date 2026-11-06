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

/// A syntax or schema problem in a document. Positions are one-based; the
/// marker spans `column..end_column` on `line`. Problems that concern the
/// whole document (such as an invalid schema) have no position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub end_column: Option<usize>,
    pub message: String,
}

/// A JSON Schema that failed to compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError(pub String);

impl std::fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SchemaError {}

/// A compiled JSON Schema, cheap to clone and reusable across documents and
/// languages: the same schema validates YAML, JSON, TOML, HCL, and XML,
/// because every language parses into the same data model.
///
/// ```
/// # #[cfg(feature = "schema-validation")] {
/// use leptos_config_editor::{Language, Schema};
/// let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
/// assert!(schema.validate(Language::Yaml, "listen: 7835\n").is_empty());
/// let problems = schema.validate(Language::Toml, "listen = \"x\"\n");
/// assert_eq!(problems[0].line, Some(1));
/// # }
/// ```
#[derive(Clone)]
pub struct Schema {
    pub(super) bundle: std::sync::Arc<SchemaBundle>,
}

impl Schema {
    /// Compiles a JSON Schema document.
    pub fn compile(schema_json: &str) -> Result<Self, SchemaError> {
        build_schema(schema_json)
            .map(|bundle| Schema {
                bundle: std::sync::Arc::new(bundle),
            })
            .map_err(SchemaError)
    }

    /// Syntax and schema diagnostics for `source`, exactly as the editor
    /// shows them.
    pub fn validate(&self, language: super::Language, source: &str) -> Vec<Diagnostic> {
        validate(source, Some(&self.bundle), None, true, language.behavior())
    }
}

/// Syntax diagnostics only, for documents without a schema.
pub fn check_syntax(language: super::Language, source: &str) -> Vec<Diagnostic> {
    validate(source, None, None, true, language.behavior())
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

    #[cfg_attr(not(feature = "schema-validation"), allow(unused_variables))]
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

#[cfg(feature = "schema-validation")]
fn escape_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}
