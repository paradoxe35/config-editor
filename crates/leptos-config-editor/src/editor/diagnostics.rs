#[cfg(feature = "schema-validation")]
use jsonschema::error::{ValidationError, ValidationErrorKind};
use noyalib::{DuplicateKeyPolicy, ParserConfig};
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
                    let parent = error.instance_path().as_str();
                    unexpected.iter().find_map(|key| {
                        yaml_span_for_path(source, &format!("{parent}/{}", escape_pointer(key)))
                    })
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
fn escape_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

#[cfg(feature = "schema-validation")]
fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Maps every mapping key and sequence item in the document to its source line.
/// Paths are the same JSON-pointer segments the schema validator reports, so a
/// nested `listen` is never confused with a root `listen`.
#[cfg(feature = "schema-validation")]
fn yaml_path_lines(source: &str) -> Vec<(Vec<String>, usize, usize, usize)> {
    let mut entries = Vec::new();
    let mut stack: Vec<(usize, Vec<String>)> = Vec::new();
    let mut counts: std::collections::HashMap<(Vec<String>, usize), usize> =
        std::collections::HashMap::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let mut rest = trimmed;
        let mut content_indent = indent;

        let mut array_item_path = None;
        if rest == "-" || rest.starts_with("- ") {
            while stack.last().is_some_and(|(level, _)| *level >= indent) {
                stack.pop();
            }
            let parent = stack
                .last()
                .map(|(_, path)| path.clone())
                .unwrap_or_default();
            let count = counts.entry((parent.clone(), indent)).or_default();
            let mut item_path = parent;
            item_path.push(count.to_string());
            *count += 1;
            let after_dash = rest.strip_prefix('-').unwrap_or_default();
            let content = after_dash.trim_start();
            content_indent = indent + 1 + after_dash.len() - content.len();
            array_item_path = Some(item_path.clone());
            stack.push((indent, item_path));
            rest = content;
        }

        if rest.is_empty() {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                entries.push((path, index + 1, column, column + 1));
            }
            continue;
        }
        let Some(colon) = rest.find(':') else {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                let end_column = column + rest.chars().count();
                entries.push((path, index + 1, column, end_column));
            }
            continue;
        };
        let key_raw = &rest[..colon];
        let key = key_raw.trim().trim_matches(['\'', '"']);
        if key.is_empty() {
            continue;
        }
        while stack
            .last()
            .is_some_and(|(level, _)| *level >= content_indent)
        {
            stack.pop();
        }
        let mut path = stack
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        path.push(key.to_string());
        let column = line[..content_indent].chars().count() + 1;
        let end_column = column + key_raw.trim_end().chars().count();
        entries.push((path.clone(), index + 1, column, end_column));
        let value = rest[colon + 1..].trim();
        if value.is_empty() || value.starts_with('#') {
            stack.push((content_indent, path));
        }
    }
    entries
}

#[cfg(feature = "schema-validation")]
fn yaml_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let key = wanted.last()?.clone();
    if key.is_empty() {
        return None;
    }

    if let Some((_, line, column, end_column)) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
    {
        return Some((line, column, end_column));
    }

    // Fall back to the previous first-match behavior for paths the scanner
    // cannot represent, such as flow mappings.
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
    let wanted = pointer_segments(path);
    let line_number = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
        .map(|(_, line, _, _)| line)
        .or_else(|| yaml_span_for_path(source, path).map(|(line, _, _)| line))?;
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
    fn a_nested_key_maps_to_its_own_line_not_the_first_key_with_the_same_name() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"listen":{"type":"integer"},"websocket":{"type":"object","properties":{"listen":{"type":"integer","minimum":1}}}}}"#,
        )
        .unwrap();
        let source = "listen: 7835\nwebsocket:\n  listen: 0\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(11));
    }

    #[test]
    fn an_unexpected_nested_key_maps_to_its_own_line_not_a_root_key_with_the_same_name() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"listen":{"type":"integer"},"websocket":{"type":"object","properties":{},"additionalProperties":false}}}"#,
        )
        .unwrap();
        let source = "listen: 7835\nwebsocket:\n  listen: 0\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
    }

    #[test]
    fn a_key_inside_a_sequence_item_maps_to_its_own_line() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"weight":{"type":"integer"},"services":{"type":"object","additionalProperties":{"type":"object","properties":{"clients":{"type":"array","items":{"type":"object","properties":{"weight":{"type":"integer","minimum":1}}}}}}}}}"#,
        )
        .unwrap();
        let source = "weight: 5\nservices:\n  billing:\n    clients:\n      - name: laptop\n        weight: 0\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(6));
    }

    #[test]
    fn an_empty_sequence_item_error_points_to_the_dash_line() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"self":{"type":"object","properties":{"allow":{"type":"array","items":{"type":"string"}}}}}}"#,
        )
        .unwrap();
        let source = "self:\n  allow:\n    - \n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(7));
    }

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
