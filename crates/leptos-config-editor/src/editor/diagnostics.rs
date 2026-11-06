use super::language::ConfigLanguage;
#[cfg(feature = "schema-validation")]
use super::language::Span;
#[cfg(feature = "schema-validation")]
use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

#[cfg(feature = "schema-validation")]
const MAX_DIAGNOSTICS: usize = 20;

/// Milliseconds after the last keystroke before revalidating, so half-typed
/// states do not flash squiggles.
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

/// A syntax or schema problem in a document.
///
/// Positions are one-based; the marker spans `column..end_column` on `line`.
/// Problems that concern the whole document (such as an invalid schema) have
/// no position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// One-based line.
    pub line: Option<usize>,
    /// One-based character column where the marker starts.
    pub column: Option<usize>,
    /// One-based character column just past the marker.
    pub end_column: Option<usize>,
    /// What is wrong; schema problems start with the value's JSON pointer.
    pub message: String,
    /// How serious the problem is.
    pub severity: Severity,
}

/// How serious a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The document is invalid: a syntax error or a schema violation.
    #[default]
    Error,
    /// The document is valid but deserves attention, such as a value the host
    /// language cannot keep exactly when editing with `edit_as`.
    Warning,
}

/// A JSON Schema that failed to compile, with the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError(pub String);

impl std::fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SchemaError {}

/// A compiled JSON Schema, cheap to clone.
///
/// One schema validates documents in every language, because every language
/// parses into the same data model.
///
/// ```
/// # #[cfg(all(feature = "schema-validation", feature = "yaml", feature = "toml"))] {
/// use leptos_config_editor::{Language, Schema};
/// let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
/// assert!(schema.validate(Language::YAML, "listen: 7835\n").is_empty());
/// let problems = schema.validate(Language::TOML, "listen = \"x\"\n");
/// assert_eq!(problems[0].line, Some(1));
/// # }
/// ```
#[derive(Clone)]
pub struct Schema {
    pub(super) bundle: std::sync::Arc<SchemaBundle>,
}

impl std::fmt::Debug for Schema {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let title = self.bundle.schema.get("title").and_then(Value::as_str);
        formatter
            .debug_struct("Schema")
            .field("title", &title)
            .finish_non_exhaustive()
    }
}

/// The `schema` of a [`ConfigEditor`](crate::ConfigEditor): a JSON Schema
/// document, or a [`Schema`] compiled once and shared between editors.
///
/// Converts from `&str`, `String`, and `Schema`, so `schema=SCHEMA_JSON` and
/// `schema=compiled.clone()` both work.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SchemaSource {
    /// A JSON Schema document, compiled when the editor mounts; a compile
    /// error is shown inline.
    Json(String),
    /// An already compiled schema.
    Compiled(Schema),
}

impl From<&str> for SchemaSource {
    fn from(json: &str) -> Self {
        SchemaSource::Json(json.to_string())
    }
}

impl From<String> for SchemaSource {
    fn from(json: String) -> Self {
        SchemaSource::Json(json)
    }
}

impl From<Schema> for SchemaSource {
    fn from(schema: Schema) -> Self {
        SchemaSource::Compiled(schema)
    }
}

impl SchemaSource {
    pub(super) fn resolve(self) -> Result<std::sync::Arc<SchemaBundle>, String> {
        match self {
            SchemaSource::Json(json) => build_schema(&json).map(std::sync::Arc::new),
            SchemaSource::Compiled(schema) => Ok(schema.bundle),
        }
    }
}

impl Schema {
    /// Compiles a JSON Schema document.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when `schema_json` is not JSON or not a valid
    /// schema.
    pub fn compile(schema_json: &str) -> Result<Self, SchemaError> {
        build_schema(schema_json)
            .map(|bundle| Schema {
                bundle: std::sync::Arc::new(bundle),
            })
            .map_err(SchemaError)
    }

    /// Syntax and schema diagnostics for `source`, as the editor shows them.
    pub fn validate(&self, language: super::Language, source: &str) -> Vec<Diagnostic> {
        validate(source, Some(&self.bundle), None, true, language.behavior())
    }

    /// Converts `source` from one language to another, reading it as this
    /// schema expects: a single XML element where the schema wants an array
    /// is a one-item array, and converts to one.
    ///
    /// # Errors
    ///
    /// As [`Language::convert`](super::Language::convert).
    pub fn convert(
        &self,
        from: super::Language,
        source: &str,
        to: super::Language,
    ) -> Result<super::Conversion, super::ConvertError> {
        super::codec::convert(
            source,
            from.behavior(),
            to.behavior(),
            None,
            Some(&self.bundle.schema),
        )
    }
}

/// Characters of a value quoted in a message before it is abbreviated.
#[cfg(feature = "schema-validation")]
const MAX_QUOTED_VALUE: usize = 80;

/// Syntax diagnostics for `source`, for documents without a schema.
///
/// ```
/// # #[cfg(feature = "json")] {
/// use leptos_config_editor::{Language, check_syntax};
/// assert!(check_syntax(Language::JSON, "{\"a\": 1}").is_empty());
/// assert_eq!(check_syntax(Language::JSON, "{\"a\": }").len(), 1);
/// # }
/// ```
pub fn check_syntax(language: super::Language, source: &str) -> Vec<Diagnostic> {
    validate(source, None, None, true, language.behavior())
}
/// `diagnostics` found in `validated`, moved to where their lines are in
/// `current`: those after the edited lines shift by the lines it added or
/// removed, those on edited lines are dropped until the next validation.
pub(super) fn shift_diagnostics(
    validated: &str,
    current: &str,
    diagnostics: &[Diagnostic],
) -> Vec<Diagnostic> {
    let count = |text: &str| text.bytes().filter(|byte| *byte == b'\n').count();
    if diagnostics.is_empty() {
        return Vec::new();
    }
    // A change within one line moves the markers after it on that line.
    if count(validated) == count(current) {
        return shift_columns(validated, current, diagnostics);
    }
    let old: Vec<&str> = validated.split('\n').collect();
    let new: Vec<&str> = current.split('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(old.len().min(new.len()) - prefix)
        .take_while(|(a, b)| a == b)
        .count();
    let delta = new.len() as isize - old.len() as isize;
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let Some(line) = diagnostic.line else {
                return Some(diagnostic.clone());
            };
            if line <= prefix {
                Some(diagnostic.clone())
            } else if line > old.len() - suffix {
                Some(Diagnostic {
                    line: Some((line as isize + delta) as usize),
                    ..diagnostic.clone()
                })
            } else {
                None
            }
        })
        .collect()
}

/// `diagnostics` with columns on the one edited line moved by the characters
/// typed or deleted before them.
fn shift_columns(validated: &str, current: &str, diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    let edited = validated
        .split('\n')
        .zip(current.split('\n'))
        .enumerate()
        .filter(|(_, (old, new))| old != new)
        .map(|(index, (old, new))| (index + 1, old, new))
        .collect::<Vec<_>>();
    let [(line, old, new)] = edited.as_slice() else {
        return diagnostics.to_vec();
    };
    let edit_column = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count()
        + 1;
    let delta = new.chars().count() as isize - old.chars().count() as isize;
    let moved = |column: usize| {
        if column >= edit_column {
            (column as isize + delta).max(edit_column as isize) as usize
        } else {
            column
        }
    };
    diagnostics
        .iter()
        .map(|diagnostic| match diagnostic.line {
            Some(at) if at == *line => Diagnostic {
                column: diagnostic.column.map(moved),
                end_column: diagnostic.end_column.map(moved),
                ..diagnostic.clone()
            },
            _ => diagnostic.clone(),
        })
        .collect()
}

/// Parses through `behavior` and maps schema violations to source spans.
pub(super) fn validate(
    source: &str,
    schema: Option<&SchemaBundle>,
    schema_error: Option<&str>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
) -> Vec<Diagnostic> {
    if !enabled {
        return Vec::new();
    }
    if source.trim().is_empty() {
        return Vec::new();
    }

    #[cfg_attr(not(feature = "schema-validation"), allow(unused_variables))]
    let document: Value =
        match super::codec::read(behavior, source, schema.map(|schema| &schema.schema)) {
            Ok(document) => document,
            Err(error) => {
                // Every error gets a marker (and F8 stop): one the parser could
                // not place goes on the first line with content.
                let fallback = error
                    .line
                    .is_none()
                    .then(|| behavior.first_span(source))
                    .flatten();
                // The marker covers the word at the error (a duplicate key),
                // not just its first character.
                let word_end = error.line.zip(error.column).and_then(|(line, column)| {
                    let text = source.lines().nth(line.checked_sub(1)?)?;
                    // A quoted key (`"min_idle"`) is covered with its quotes.
                    let rest: String = text.chars().skip(column.checked_sub(1)?).collect();
                    let quoted = usize::from(rest.starts_with('"'));
                    let body: String = rest.chars().skip(quoted).collect();
                    let word = body
                        .chars()
                        .take_while(|character| {
                            character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
                        })
                        .count();
                    let closing = usize::from(quoted == 1 && body.chars().nth(word) == Some('"'));
                    let width = quoted + word + closing;
                    (width > 1).then_some(column + width)
                });
                return vec![Diagnostic {
                    line: error.line.or(fallback.map(|span| span.line)),
                    column: error.column.or(fallback.map(|span| span.start_column)),
                    end_column: word_end
                        .or(error.column.map(|column| column + 1))
                        .or(fallback.map(|span| span.end_column)),
                    message: error.message,
                    severity: Severity::Error,
                }];
            }
        };

    if let Some(error) = schema_error {
        return vec![Diagnostic {
            line: None,
            column: None,
            end_column: None,
            message: error.to_string(),
            severity: Severity::Error,
        }];
    }

    #[cfg(feature = "schema-validation")]
    if let Some(schema) = schema {
        return schema
            .validator
            .iter_errors(&document)
            .flat_map(|top| {
                specific_errors(&top)
                    .into_iter()
                    .flat_map(|error| {
                        let path = error.instance_path().as_str().to_string();
                        let message = match error.kind() {
                            ValidationErrorKind::AnyOf { context }
                            | ValidationErrorKind::OneOfNotValid { context } => {
                                expected_types(error, context).unwrap_or_else(|| error.to_string())
                            }
                            // Every allowed value, not "or 2 other candidates".
                            ValidationErrorKind::Enum { options } => match options.as_array() {
                                Some(options) => format!(
                                    "{} is not one of {}",
                                    error.instance(),
                                    options
                                        .iter()
                                        .map(Value::to_string)
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                None => error.to_string(),
                            },
                            _ => error.to_string().replacen(
                                " is not of types ",
                                " is not of type ",
                                1,
                            ),
                        };
                        // A large value is abbreviated rather than quoted whole.
                        let instance = error.instance().to_string();
                        let message = if instance.chars().count() > MAX_QUOTED_VALUE {
                            let short: String = instance.chars().take(MAX_QUOTED_VALUE).collect();
                            message.replacen(&instance, &format!("{short}…"), 1)
                        } else {
                            message
                        };
                        let message = if path.is_empty() {
                            message
                        } else {
                            format!("{path}: {message}")
                        };
                        // Each unknown key gets its own marker and message, so every
                        // one is underlined and reachable with F8.
                        let locations: Vec<(Option<Span>, String)> =
                            if let ValidationErrorKind::AdditionalProperties { unexpected } =
                                error.kind()
                            {
                                let parent = error.instance_path().as_str();
                                // Only as many as can be shown are located: each is a scan.
                                unexpected
                                    .iter()
                                    .take(MAX_DIAGNOSTICS)
                                    .map(|key| {
                                        let pointer = format!("{parent}/{}", escape_pointer(key));
                                        let span = super::codec::inline_location(
                                            behavior, source, &pointer, true,
                                        )
                                        .or_else(|| behavior.key_span(source, &pointer));
                                        let place = if parent.is_empty() { "/" } else { parent };
                                        let message = if key == "#text" {
                                            // XML mixed content: text beside child elements.
                                            format!(
                                                "{place}: text is not allowed here, only elements"
                                            )
                                        } else {
                                            format!("{place}: '{key}' is not allowed here")
                                        };
                                        (span, message)
                                    })
                                    .collect()
                            } else {
                                // A missing property is reported on the object's key.
                                let locate = |path: &str| {
                                    if matches!(error.kind(), ValidationErrorKind::Required { .. })
                                    {
                                        super::codec::inline_location(behavior, source, path, true)
                                            .or_else(|| behavior.key_span(source, path))
                                            .or_else(|| behavior.value_span(source, path))
                                    } else {
                                        super::codec::inline_location(behavior, source, path, false)
                                            .or_else(|| behavior.value_span(source, path))
                                            .or_else(|| behavior.key_span(source, path))
                                    }
                                };
                                // A lone block or element read as a one-item list
                                // is addressed `…/0/…` but written without it.
                                let location = locate(&path).or_else(|| {
                                    (behavior.repetition() != super::language::Repetition::None
                                        && path.contains("/0"))
                                    .then(|| {
                                        let lone = path.replace("/0/", "/");
                                        locate(lone.strip_suffix("/0").unwrap_or(&lone))
                                    })
                                    .flatten()
                                });
                                vec![(location, message)]
                            };
                        locations
                            .into_iter()
                            .map(|(location, message)| {
                                let location = location.or_else(|| behavior.first_span(source));
                                Diagnostic {
                                    line: location.map(|location| location.line),
                                    column: location.map(|location| location.start_column),
                                    end_column: location.map(|location| location.end_column),
                                    message,
                                    severity: Severity::Error,
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .take(MAX_DIAGNOSTICS)
            .collect();
    }
    let _ = schema;
    Vec::new()
}

/// For an `anyOf` whose every branch wants another type: `true is not of
/// type "integer", "string", "null"`, instead of "not valid under any of the
/// schemas".
#[cfg(feature = "schema-validation")]
fn expected_types(
    error: &ValidationError<'_>,
    branches: &[Vec<ValidationError<'_>>],
) -> Option<String> {
    let mut types: Vec<String> = Vec::new();
    for nested in branches.iter().flatten() {
        if !matches!(nested.kind(), ValidationErrorKind::Type { .. }) {
            return None;
        }
        let message = nested.to_string();
        let listed = message
            .split_once(" is not of type")
            .map(|(_, rest)| rest)?;
        let listed = listed.strip_prefix('s').unwrap_or(listed);
        for name in listed.split(',') {
            let name = name.trim().to_string();
            if !name.is_empty() && !types.contains(&name) {
                types.push(name);
            }
        }
    }
    (!types.is_empty()).then(|| format!("{} is not of type {}", error.instance(), types.join(", ")))
}

/// The errors to show for `error`: inside `anyOf`/`oneOf`, every error of the
/// branch the value was meant for. A branch that fails only because
/// the value has another type (`null` for an object) is skipped, so a typo in
/// an object reports the typo rather than "not valid under any of the schemas".
#[cfg(feature = "schema-validation")]
fn specific_errors<'e, 'a>(error: &'e ValidationError<'a>) -> Vec<&'e ValidationError<'a>> {
    let branches = match error.kind() {
        ValidationErrorKind::AnyOf { context } | ValidationErrorKind::OneOfNotValid { context } => {
            context
        }
        _ => return vec![error],
    };
    let path = error.instance_path().as_str();
    let wrong_type = |branch: &Vec<ValidationError<'a>>| {
        branch.iter().all(|nested| {
            matches!(nested.kind(), ValidationErrorKind::Type { .. })
                && nested.instance_path().as_str() == path
        })
    };
    let depth = |error: &ValidationError<'_>| error.instance_path().as_str().matches('/').count();
    let deepest_in = |branch: &'e Vec<ValidationError<'a>>| {
        branch
            .iter()
            .flat_map(specific_errors)
            .map(&depth)
            .max()
            .unwrap_or(0)
    };
    let meant: Vec<&'e Vec<ValidationError<'a>>> = branches
        .iter()
        .filter(|branch| !wrong_type(branch))
        .collect();
    // The branch the value was meant for: every one of its errors is shown,
    // so a second problem is not hidden behind the first.
    if let Some(branch) = meant.into_iter().max_by_key(|branch| deepest_in(branch)) {
        return branch.iter().flat_map(specific_errors).collect();
    }
    // Without one, only a deeper error says more than "wrong type".
    match branches
        .iter()
        .flatten()
        .flat_map(specific_errors)
        .max_by_key(|nested| depth(nested))
    {
        Some(deepest) if depth(deepest) > depth(error) => vec![deepest],
        _ => vec![error],
    }
}

#[cfg(feature = "schema-validation")]
fn escape_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

#[cfg(all(test, feature = "schema-validation", feature = "yaml"))]
mod message_tests {
    use super::super::Language;
    use super::Schema;

    fn messages(yaml: &str) -> Vec<String> {
        Schema::compile(include_str!("../../tests/fixtures/burrow.schema.json"))
            .unwrap()
            .validate(Language::YAML, yaml)
            .into_iter()
            .map(|problem| problem.message)
            .collect()
    }

    #[test]
    fn a_typo_in_an_optional_object_names_the_typo() {
        assert_eq!(
            messages("web: {listen: 443}\n"),
            ["/web: 'listen' is not allowed here"]
        );
    }

    #[test]
    fn a_wrong_type_lists_every_accepted_type() {
        assert_eq!(
            messages("listen: true\n"),
            [r#"/listen: true is not of type "integer", "string""#]
        );
    }

    #[test]
    fn every_problem_in_an_optional_object_is_reported() {
        let found = messages(
            "services:\n  web:\n    target: 80\n    health_check: {interval_secs: 0, bogus: 1}\n",
        );
        assert!(
            found
                .iter()
                .any(|message| message.contains("'bogus' is not allowed")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|message| message.contains("minimum")),
            "{found:?}"
        );
    }

    #[test]
    fn a_wrong_choice_lists_every_allowed_value() {
        assert_eq!(
            messages("services:\n  web:\n    target: 80\n    mode: fastest\n"),
            [
                r#"/services/web/mode: "fastest" is not one of "failover", "balance", "least_connections", "ip_hash""#
            ]
        );
    }
}

#[cfg(test)]
mod shift_tests {
    use super::{Diagnostic, Severity, shift_diagnostics};

    fn on(line: usize) -> Diagnostic {
        Diagnostic {
            line: Some(line),
            column: Some(1),
            end_column: Some(2),
            message: String::new(),
            severity: Severity::Error,
        }
    }

    fn lines(found: Vec<Diagnostic>) -> Vec<Option<usize>> {
        found
            .into_iter()
            .map(|diagnostic| diagnostic.line)
            .collect()
    }

    #[test]
    fn markers_below_an_inserted_line_move_down() {
        let shifted = shift_diagnostics("a\nb\nc", "a\n\nb\nc", &[on(1), on(3)]);
        assert_eq!(lines(shifted), [Some(1), Some(4)]);
    }

    #[test]
    fn markers_below_a_removed_line_move_up_and_the_removed_ones_go() {
        let shifted = shift_diagnostics("a\nb\nc\nd", "a\nd", &[on(2), on(4)]);
        assert_eq!(lines(shifted), [Some(2)]);
    }

    #[test]
    fn typing_within_a_line_moves_nothing_on_other_lines() {
        let shifted = shift_diagnostics("a: 1\nb: x", "a: 12\nb: x", &[on(2)]);
        assert_eq!(lines(shifted), [Some(2)]);
    }

    #[test]
    fn typing_before_a_marker_on_its_line_moves_its_columns() {
        let mut marker = on(1);
        marker.column = Some(9);
        marker.end_column = Some(10);
        let shifted = shift_diagnostics("limits: {max: 0}", "limits:   {max: 0}", &[marker]);
        assert_eq!(
            (shifted[0].column, shifted[0].end_column),
            (Some(11), Some(12))
        );
    }
}

#[cfg(all(test, feature = "schema-validation", feature = "yaml"))]
mod scale_tests {
    use super::super::Language;
    use super::Schema;

    #[test]
    fn many_unknown_keys_stay_fast_and_messages_stay_short() {
        let schema =
            Schema::compile(include_str!("../../tests/fixtures/burrow.schema.json")).unwrap();
        let document: String = (0..2000)
            .map(|index| format!("unknown_{index}: 1\n"))
            .collect();
        let started = std::time::Instant::now();
        let problems = schema.validate(Language::YAML, &document);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(300),
            "{:?}",
            started.elapsed()
        );
        assert!(problems.iter().all(|problem| problem.message.len() < 200));
        let services: String = format!(
            "services:\n{}",
            (0..200)
                .map(|index| format!("  - name: s{index}\n"))
                .collect::<String>()
        );
        let problems = schema.validate(Language::YAML, &services);
        assert!(
            problems
                .iter()
                .all(|problem| problem.message.chars().count() < 300),
            "{problems:?}"
        );
    }
}
