#[cfg(feature = "schema-validation")]
use jsonschema::error::{ValidationError, ValidationErrorKind};
use noyalib::{DuplicateKeyPolicy, ParserConfig};
use serde_json::Value;

#[cfg(feature = "schema-validation")]
const MAX_DIAGNOSTICS: usize = 20;

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
pub(super) fn validate_yaml(
    source: &str,
    schema: Option<&SchemaBundle>,
    schema_error: Option<&str>,
    enabled: bool,
) -> Vec<Diagnostic> {
    if !enabled {
        return Vec::new();
    }
    if source.trim().is_empty() {
        return Vec::new();
    }

    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    let _document: Value = match noyalib::from_str_with_config(source, &parser_config) {
        Ok(document) => document,
        Err(error) => {
            let line_count = source.split('\n').count().max(1);
            let location = error.location();
            let line = location
                .as_ref()
                .map(|location| location.line().min(line_count));
            let column = location.as_ref().map(|location| location.column().max(1));
            let mut message = error.to_string();
            if let Some(location) = location
                && location.line() > line_count
            {
                message = message.replace(
                    &format!("at line {} column {}", location.line(), location.column()),
                    "at end of input",
                );
            }
            return vec![Diagnostic {
                line,
                column,
                end_column: column.map(|column| column + 1),
                message,
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
            .iter_errors(&_document)
            .take(MAX_DIAGNOSTICS)
            .map(|error| {
                let (path, message) = most_specific_validation_error(&error);
                let location = if let ValidationErrorKind::AdditionalProperties { unexpected } =
                    error.kind()
                {
                    unexpected
                        .iter()
                        .find_map(|key| yaml_span_for_path(source, &format!("/{key}")))
                } else {
                    yaml_value_span_for_path(source, &path)
                        .or_else(|| yaml_span_for_path(source, &path))
                }
                .or_else(|| first_yaml_span(source));
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
fn yaml_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let key = path
        .rsplit('/')
        .next()?
        .replace("~1", "/")
        .replace("~0", "~");
    if key.is_empty() {
        return None;
    }

    source.lines().enumerate().find_map(|(index, line)| {
        let indent = line.len() - line.trim_start().len();
        let (candidate, _) = line.trim_start().split_once(':')?;
        if candidate.trim_matches(['\'', '"']) != key {
            return None;
        }
        let candidate_offset = line[indent..].find(candidate)?;
        let column = line[..indent].chars().count()
            + line[indent..indent + candidate_offset].chars().count()
            + 1;
        Some((index + 1, column, column + candidate.chars().count()))
    })
}

#[cfg(feature = "schema-validation")]
fn yaml_value_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (line_number, _, _) = yaml_span_for_path(source, path)?;
    let line = source.lines().nth(line_number.saturating_sub(1))?;
    let (key, raw_value) = line.split_once(':')?;
    let value_start = raw_value.len() - raw_value.trim_start().len();
    let value = raw_value.trim();
    let column = key.chars().count() + raw_value[..value_start].chars().count() + 2;
    if value.is_empty() || value.starts_with('#') {
        return Some((line_number, column, column + 1));
    }

    Some((line_number, column, column + value.chars().count()))
}

#[cfg(feature = "schema-validation")]
fn first_yaml_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}

#[cfg(all(test, feature = "schema-validation"))]
mod tests {
    use super::*;

    #[test]
    fn empty_schema_value_points_after_the_colon_instead_of_at_the_key() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"websocket":{"type":"object","properties":{"listen":{"type":"string"}}}}}"#,
        )
        .unwrap();
        let source = "websocket:\n  listen: \n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(11));
        assert_eq!(diagnostics[0].end_column, Some(12));
    }

    #[test]
    fn empty_schema_value_with_comment_points_to_the_value_slot() {
        let schema =
            build_schema(r#"{"type":"object","properties":{"listen":{"type":"string"}}}"#).unwrap();
        let source = "listen: # set a port\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(1));
        assert_eq!(diagnostics[0].column, Some(9));
        assert_eq!(diagnostics[0].end_column, Some(10));
    }

    #[test]
    fn nested_any_of_errors_point_to_the_specific_empty_value() {
        let schema = build_schema(
            r##"{
                "type":"object",
                "properties":{
                    "websocket":{
                        "anyOf":[
                            {"$ref":"#/$defs/WebSocket"},
                            {"type":"null"}
                        ]
                    }
                },
                "$defs":{
                    "WebSocket":{
                        "type":"object",
                        "properties":{
                            "listen":{"anyOf":[{"type":"integer"},{"type":"string"}]}
                        },
                        "required":["listen"]
                    }
                }
            }"##,
        )
        .unwrap();
        let source = "websocket:\n  listen: \n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(11));
        assert!(diagnostics[0].message.contains("/websocket/listen"));
    }
}
