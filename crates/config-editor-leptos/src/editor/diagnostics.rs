use super::language::ConfigLanguage;
#[cfg(feature = "schema-validation")]
use super::language::Span;
#[cfg(feature = "schema-validation")]
use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

use crate::EditorStrings;
use crate::strings::{default_strings, fill};

/// The most problems of one kind (the schema's, the secret placeholders')
/// shown and underlined; past it, a note says more follow.
pub(super) const MAX_DIAGNOSTICS: usize = 20;

/// The [`Diagnostic::code`] of the note that the problem list was cut.
pub(super) const MORE_PROBLEMS_CODE: &str = "config-editor/more-problems";

/// The [`MAX_DIAGNOSTICS`] most serious of `diagnostics` (errors before
/// warnings, each kind in the order found, kept in that order), and when
/// there are more, one without a position saying so (the footer shows it),
/// so a cut list is never taken for the whole one. The note has the
/// severity of the most serious problem it cut: the footer's count of that
/// kind says "20+".
pub(super) fn capped(mut diagnostics: Vec<Diagnostic>, strings: &EditorStrings) -> Vec<Diagnostic> {
    if diagnostics.len() > MAX_DIAGNOSTICS {
        let mut order: Vec<usize> = (0..diagnostics.len()).collect();
        order.sort_by_key(|index| diagnostics[*index].severity as u8);
        let mut kept = vec![false; diagnostics.len()];
        for index in &order[..MAX_DIAGNOSTICS] {
            kept[*index] = true;
        }
        let severity = order[MAX_DIAGNOSTICS..]
            .iter()
            .map(|index| diagnostics[*index].severity)
            .min_by_key(|severity| *severity as u8)
            .unwrap_or(Severity::Warning);
        let mut index = 0;
        diagnostics.retain(|_| {
            index += 1;
            kept[index - 1]
        });
        diagnostics.push(Diagnostic {
            line: None,
            column: None,
            end_column: None,
            message: fill(&strings.problems_more, &[("count", &MAX_DIAGNOSTICS)]),
            severity,
            code: Some(MORE_PROBLEMS_CODE.to_string()),
            ..Diagnostic::default()
        });
    }
    diagnostics
}

/// Milliseconds after the last keystroke before revalidating, so half-typed
/// states do not flash squiggles.
pub(super) const DIAGNOSTICS_DEBOUNCE_MS: i32 = 250;

pub(super) struct SchemaBundle {
    pub(super) schema: Value,
    /// What the editor cannot use in the schema (see [`Schema::warnings`]).
    pub(super) warnings: Vec<String>,
    /// Some property is `deprecated`: documents are checked for its key.
    pub(super) has_deprecated: bool,
    /// Some field is `readOnly`, `writeOnly` or `x-secret`: documents are
    /// walked for them.
    pub(super) has_marks: bool,
    #[cfg(feature = "schema-validation")]
    validator: jsonschema::Validator,
}

/// The compiled schema, or why `schema_text` is no JSON Schema (worded by
/// the `invalidSchema` text where it is shown).
pub(super) fn build_schema(schema_text: &str) -> Result<SchemaBundle, String> {
    let schema: Value = serde_json::from_str(schema_text).map_err(|error| error.to_string())?;
    #[cfg(feature = "schema-validation")]
    let validator = jsonschema::validator_for(
        super::false_schema::for_validation(&schema)
            .as_ref()
            .unwrap_or(&schema),
    )
    .map_err(|error| error.to_string())?;
    let warnings = super::completion::unsupported_patterns(&schema);
    let has_deprecated = super::completion::mentions_deprecated(&schema);
    let has_marks = super::protect::mentions_marks(&schema);
    Ok(SchemaBundle {
        schema,
        warnings,
        has_deprecated,
        has_marks,
        #[cfg(feature = "schema-validation")]
        validator,
    })
}

/// A problem in a document: the editor's own (syntax, schema, Edit-as) or
/// one the host reported ([`HostDiagnostic`](crate::HostDiagnostic)).
///
/// Positions are one-based; the marker spans `column..end_column` on `line`,
/// or from `column` on `line` to `end_column` on `end_line` when it covers
/// several lines (a host's range). Problems that concern the whole document
/// (such as an invalid schema) have no position.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Diagnostic {
    /// One-based line.
    pub line: Option<usize>,
    /// One-based character column where the marker starts.
    pub column: Option<usize>,
    /// One-based character column just past the marker (on `end_line` when
    /// it is set).
    pub end_column: Option<usize>,
    /// One-based line the marker ends on, when it covers several lines;
    /// `None` when it ends on `line`. Only a host's range covers several.
    pub end_line: Option<usize>,
    /// What is wrong; schema problems start with the value's JSON pointer.
    pub message: String,
    /// How serious the problem is.
    pub severity: Severity,
    /// Who found it.
    pub origin: DiagnosticOrigin,
    /// A machine-readable code, from the host.
    pub code: Option<String>,
    /// Who reported it, for a host problem (`"host"` unless it said).
    pub source: Option<String>,
    /// A page explaining the problem, from the host.
    pub docs_url: Option<String>,
    /// A key the schema marks `deprecated`: the marker is on the key, which
    /// is struck through.
    pub deprecated: bool,
}

impl Diagnostic {
    /// The line the marker ends on: `end_line`, else `line`.
    pub(crate) fn last_line(&self) -> Option<usize> {
        self.end_line.or(self.line)
    }
}

/// Who found a [`Diagnostic`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DiagnosticOrigin {
    /// The editor: syntax, schema, or a value Edit-as cannot keep.
    #[default]
    Editor,
    /// The host, through `host_diagnostics`.
    Host,
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
    /// Something worth knowing.
    Info,
    /// A suggestion, shown discreetly.
    Hint,
}

impl Severity {
    /// The lowercase name: `"error"`, `"warning"`, `"info"` or `"hint"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
            Severity::Hint => "hint",
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    /// A lowercase name, as [`as_str`](Self::as_str) writes it
    /// (`"information"` reads as info).
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Severity::Error),
            "warning" => Ok(Severity::Warning),
            "info" | "information" => Ok(Severity::Info),
            "hint" => Ok(Severity::Hint),
            _ => Err(format!(
                "unknown severity \"{name}\" (expected error, warning, info or hint)"
            )),
        }
    }
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
/// use config_editor_leptos::{Language, Schema};
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

impl Schema {
    /// Whether both are handles on one compiled schema (clones of each
    /// other), not merely equal ones.
    pub(crate) fn shares(&self, other: &Schema) -> bool {
        std::sync::Arc::ptr_eq(&self.bundle, &other.bundle)
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
            .map_err(|error| {
                SchemaError(fill(
                    &default_strings().invalid_schema,
                    &[("error", &error)],
                ))
            })
    }

    /// What the editor cannot use in this schema, one message each, such as
    /// a `patternProperties` regex its engine cannot compile (keys it was
    /// meant to match get no completions or hover docs). The editor passes
    /// each to `on_schema_warning` when it mounts.
    ///
    /// ```
    /// use config_editor_leptos::Schema;
    /// let schema = Schema::compile(r#"{"patternProperties":{"^(?!x-)":{}}}"#).unwrap();
    /// assert_eq!(schema.warnings().len(), 1);
    /// ```
    pub fn warnings(&self) -> &[String] {
        &self.bundle.warnings
    }

    /// Syntax and schema diagnostics for `source`, as the editor shows them
    /// (the editor's own messages in English, the `strings` defaults).
    pub fn validate(&self, language: super::Language, source: &str) -> Vec<Diagnostic> {
        headless_validate(source, Some(&self.bundle), language.behavior())
    }

    /// The suggestions the editor lists at `cursor` (a UTF-16 offset in
    /// `source`) on `Ctrl+Space`, best first: the keys and values this schema
    /// allows there, written as `language` writes them. Accept one with
    /// [`Suggestion::accept`](super::Suggestion::accept). A custom language
    /// checks its completion methods with it, no browser needed.
    ///
    /// ```
    /// # #[cfg(feature = "toml")] {
    /// use config_editor_leptos::{Language, Schema};
    /// let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
    /// let offered = schema.suggestions(Language::TOML, "lis", 3);
    /// assert_eq!(offered[0].label(), "listen");
    /// assert_eq!(offered[0].accept("lis").0, "listen = ");
    /// # }
    /// ```
    pub fn suggestions(
        &self,
        language: super::Language,
        source: &str,
        cursor: usize,
    ) -> Vec<super::Suggestion> {
        let source = super::line_breaks::as_lf(source);
        super::completion::schema_suggestions(&self.bundle, language.behavior(), &source, cursor)
    }

    /// The docs the hover card shows for the key at `position` in `source`
    /// (or for the key of a value the schema documents), as `language` reads
    /// it; `None` where no schema key is.
    pub fn hover(
        &self,
        language: super::Language,
        source: &str,
        position: super::TextPosition,
    ) -> Option<super::HoverDocs> {
        let source = super::line_breaks::as_lf(source);
        super::completion::schema_hover(&self.bundle, language.behavior(), &source, position)
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
        super::codec::convert_away(
            source,
            from.behavior(),
            to.behavior(),
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
/// use config_editor_leptos::{Language, check_syntax};
/// assert!(check_syntax(Language::JSON, "{\"a\": 1}").is_empty());
/// assert_eq!(check_syntax(Language::JSON, "{\"a\": }").len(), 1);
/// # }
/// ```
pub fn check_syntax(language: super::Language, source: &str) -> Vec<Diagnostic> {
    headless_validate(source, None, language.behavior())
}

/// [`validate`] of a text from outside the editor, its line breaks read as
/// the editor reads them (see `line_breaks`): every position is one in
/// `source`, and a lone CR the language rejects is its problem.
fn headless_validate(
    source: &str,
    schema: Option<&SchemaBundle>,
    behavior: &dyn ConfigLanguage,
) -> Vec<Diagnostic> {
    let read = super::line_breaks::as_lf(source);
    let mut problems = validate(&read, schema, None, true, behavior);
    if let Some(rejected) = super::line_breaks::rejected_lone_cr_problem(behavior, source) {
        let at = problems
            .iter()
            .position(|problem| (problem.line, problem.column) > (rejected.line, rejected.column))
            .unwrap_or(problems.len());
        problems.insert(at, rejected);
    }
    problems
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
    let moved = |line: usize| (line as isize + delta) as usize;
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let Some(line) = diagnostic.line else {
                return Some(diagnostic.clone());
            };
            if line <= prefix {
                // A marker over several lines ends where its end moved; one
                // ending in the edited lines, at their last line.
                let end_line = diagnostic.end_line.map(|end| {
                    if end > old.len() - suffix {
                        moved(end)
                    } else if end > prefix {
                        (new.len() - suffix).max(line)
                    } else {
                        end
                    }
                });
                Some(Diagnostic {
                    end_line: end_line.filter(|end| *end > line),
                    ..diagnostic.clone()
                })
            } else if line > old.len() - suffix {
                Some(Diagnostic {
                    line: Some(moved(line)),
                    end_line: diagnostic.end_line.map(moved),
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
        .map(|diagnostic| {
            let starts_here = diagnostic.line == Some(*line);
            let ends_here = diagnostic.last_line() == Some(*line);
            Diagnostic {
                column: diagnostic.column.map(
                    |column| {
                        if starts_here { moved(column) } else { column }
                    },
                ),
                end_column: diagnostic.end_column.map(
                    |end| {
                        if ends_here { moved(end) } else { end }
                    },
                ),
                ..diagnostic.clone()
            }
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
    validate_with(
        source,
        schema,
        schema_error,
        enabled,
        behavior,
        &[],
        default_strings(),
    )
}

/// [`validate`], accepting a field whose whole value is one of `accepted`
/// in its own field (the host's secret placeholders, each with the field it
/// stands for the current secret of) whatever its schema says: no `pattern`, `format`, length or type
/// problem is reported on it. Messages never quote a secret field's value.
pub(super) fn validate_with(
    source: &str,
    schema: Option<&SchemaBundle>,
    schema_error: Option<&str>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
    accepted: &[(String, String)],
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if !enabled {
        return Vec::new();
    }
    if source.trim().is_empty() {
        return Vec::new();
    }

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
                    ..Diagnostic::default()
                }];
            }
        };

    if let Some(error) = schema_error {
        return vec![Diagnostic {
            line: None,
            column: None,
            end_column: None,
            message: fill(&strings.invalid_schema, &[("error", &error)]),
            severity: Severity::Error,
            ..Diagnostic::default()
        }];
    }

    let deprecated = schema
        .map(|schema| deprecation_warnings(&document, schema, source, behavior, strings))
        .unwrap_or_default();
    // Without `schema-validation` no check could reject a placeholder.
    #[cfg(not(feature = "schema-validation"))]
    let _ = accepted;
    #[cfg(feature = "schema-validation")]
    if let Some(schema) = schema {
        let placeholders = accepted_pointers(&document, accepted);
        let secrets = if schema.has_marks {
            super::protect::secret_pointers(&document, &schema.schema)
        } else {
            Vec::new()
        };
        let inside = |path: &str, pointers: &[String]| {
            pointers
                .iter()
                .any(|pointer| path == pointer || path.starts_with(&format!("{pointer}/")))
        };
        // Infinities and NaN are numbers to the schema (see `non_finite`).
        use super::codec::non_finite::{StandIn, for_validation};
        let validated = for_validation(&document, StandIn::Bounds);
        let restore = matches!(validated, std::borrow::Cow::Owned(_));
        // No one stand-in fails every check an infinity or NaN fails: the
        // other passes add the checks they stand for.
        let further: Vec<(StandIn, std::borrow::Cow<'_, Value>)> = if restore {
            StandIn::ALL[1..]
                .iter()
                .map(|pass| (*pass, for_validation(&document, *pass)))
                .collect()
        } else {
            Vec::new()
        };
        // The errors shown (enough for the cap), then where each is
        // written: the spans of their values read at once (see
        // `paths::with_spans`), not a read of the document each.
        let mut tops = Vec::new();
        let mut shown = 0;
        for top in schema.validator.iter_errors(&validated) {
            shown += specific_errors(&top)
                .iter()
                .filter(|error| !inside(error.instance_path().as_str(), &placeholders))
                .count();
            tops.push(top);
            if shown > MAX_DIAGNOSTICS {
                break;
            }
        }
        let mut found: std::collections::HashSet<(String, String)> = tops
            .iter()
            .flat_map(|top| specific_errors(top))
            .map(|error| {
                let keyword = error.kind().keyword().to_string();
                (error.instance_path().as_str().to_string(), keyword)
            })
            .collect();
        for (pass, value) in &further {
            for error in schema.validator.iter_errors(value) {
                let key = (
                    error.instance_path().as_str().to_string(),
                    error.kind().keyword().to_string(),
                );
                if shown <= MAX_DIAGNOSTICS
                    && pass.reports(error.instance(), error.kind())
                    && !inside(&key.0, &placeholders)
                    && found.insert(key)
                {
                    shown += 1;
                    tops.push(error);
                }
            }
        }
        let pointers: Vec<String> = tops
            .iter()
            .flat_map(|top| specific_errors(top))
            .map(|error| error.instance_path().as_str().to_string())
            .collect();
        let mut diagnostics = super::paths::with_spans(behavior, source, &pointers, || {
            tops.iter()
                .flat_map(|top| {
                    specific_errors(top)
                        .into_iter()
                        .flat_map(|error| {
                            let path = error.instance_path().as_str().to_string();
                            if inside(&path, &placeholders) {
                                return Vec::new();
                            }
                            // The schema's docs for the value (`x-docs-url`):
                            // its problem links to them, as its hover does.
                            let docs = super::completion::docs_url_at(
                                &schema.schema,
                                &path,
                                behavior.repetition() != super::language::Repetition::None,
                            );
                            let message = match error.kind() {
                                ValidationErrorKind::AnyOf { context }
                                | ValidationErrorKind::OneOfNotValid { context } => {
                                    expected_types(error, context, strings)
                                        .unwrap_or_else(|| error.to_string())
                                }
                                // Every allowed value, not "or 2 other candidates".
                                ValidationErrorKind::Enum { options } => match options.as_array() {
                                    Some(options) => fill(
                                        &strings.not_one_of,
                                        &[
                                            ("value", &error.instance()),
                                            (
                                                "allowed",
                                                &options
                                                    .iter()
                                                    .map(Value::to_string)
                                                    .collect::<Vec<_>>()
                                                    .join(", "),
                                            ),
                                        ],
                                    ),
                                    None => error.to_string(),
                                },
                                ValidationErrorKind::Type { .. } => {
                                    let message = error.to_string();
                                    listed_types(&message)
                                        .map(|types| {
                                            fill(
                                                &strings.wrong_type,
                                                &[
                                                    ("value", &error.instance()),
                                                    ("types", &types.join(", ")),
                                                ],
                                            )
                                        })
                                        .unwrap_or(message)
                                }
                                ValidationErrorKind::FalseSchema => {
                                    super::false_schema::message(&error.instance().to_string())
                                }
                                _ => error.to_string(),
                            };
                            // A large value is abbreviated rather than quoted whole.
                            let instance = error.instance().to_string();
                            let message = if inside(&path, &secrets) {
                                message.replace(&instance, &strings.secret_value)
                            } else if instance.chars().count() > MAX_QUOTED_VALUE {
                                let short: String =
                                    instance.chars().take(MAX_QUOTED_VALUE).collect();
                                message.replacen(&instance, &format!("{short}…"), 1)
                            } else {
                                message
                            };
                            let message = if restore {
                                super::codec::non_finite::restore_in_message(&message)
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
                                        .take(MAX_DIAGNOSTICS + 1)
                                        .map(|key| {
                                            let pointer =
                                                format!("{parent}/{}", escape_pointer(key));
                                            let span = super::codec::inline_location(
                                                behavior, source, &pointer, true,
                                            )
                                            .or_else(|| behavior.key_span(source, &pointer));
                                            let message = if key == "#text" {
                                                // XML mixed content: text beside child elements.
                                                strings.text_not_allowed.clone()
                                            } else {
                                                fill(&strings.key_not_allowed, &[("key", key)])
                                            };
                                            // At the root, no pointer, as for the others.
                                            let message = if parent.is_empty() {
                                                message
                                            } else {
                                                format!("{parent}: {message}")
                                            };
                                            (span, message)
                                        })
                                        .collect()
                                } else {
                                    // A non-empty object or list may be written as an
                                    // indented block below its key.
                                    let written_as_block = match error.instance().as_ref() {
                                        serde_json::Value::Object(map) => !map.is_empty(),
                                        serde_json::Value::Array(items) => !items.is_empty(),
                                        _ => false,
                                    };
                                    // A missing property is reported on the object's key.
                                    let location = super::paths::problem_span(
                                        behavior,
                                        source,
                                        &path,
                                        matches!(
                                            error.kind(),
                                            ValidationErrorKind::Required { .. }
                                        ),
                                        written_as_block,
                                    );
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
                                        docs_url: docs.clone(),
                                        ..Diagnostic::default()
                                    }
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .take(MAX_DIAGNOSTICS + 1)
                .collect::<Vec<_>>()
        });
        diagnostics.extend(deprecated);
        return capped(diagnostics, strings);
    }
    deprecated
}

/// Pointers of the values of `document` that are one of `accepted`.
#[cfg(feature = "schema-validation")]
fn accepted_pointers(document: &Value, accepted: &[(String, String)]) -> Vec<String> {
    if accepted.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut stack = vec![(String::new(), document)];
    while let Some((pointer, value)) = stack.pop() {
        match value {
            Value::String(text)
                if accepted.iter().any(|(token, field)| {
                    token == text && super::paths::same_field(field, &pointer)
                }) =>
            {
                found.push(pointer)
            }
            Value::Object(map) => stack.extend(
                map.iter()
                    .map(|(key, child)| (format!("{pointer}/{}", escape_pointer(key)), child)),
            ),
            Value::Array(items) => stack.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, child)| (format!("{pointer}/{index}"), child)),
            ),
            _ => {}
        }
    }
    found
}

/// Most deprecated keys marked in one document.
const MAX_DEPRECATION_WARNINGS: usize = 20;

/// A warning on each key of `document` its schema marks deprecated, with
/// what to use instead when the schema says.
fn deprecation_warnings(
    document: &Value,
    bundle: &SchemaBundle,
    source: &str,
    behavior: &dyn ConfigLanguage,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if !bundle.has_deprecated {
        return Vec::new();
    }
    super::completion::deprecated_keys(document, &bundle.schema, MAX_DEPRECATION_WARNINGS)
        .into_iter()
        .filter_map(|found| {
            // The key alone, struck through as VS Code does.
            let span = behavior.key_span(source, &found.pointer).or_else(|| {
                super::codec::inline_location(behavior, source, &found.pointer, true)
            })?;
            let message = match &found.message {
                Some(instead) => fill(
                    &strings.key_deprecated_instead,
                    &[
                        ("pointer", &found.pointer),
                        ("key", &found.key),
                        ("message", instead),
                    ],
                ),
                None => fill(
                    &strings.key_deprecated,
                    &[("pointer", &found.pointer), ("key", &found.key)],
                ),
            };
            Some(Diagnostic {
                line: Some(span.line),
                column: Some(span.start_column),
                end_column: Some(span.end_column),
                message,
                severity: Severity::Warning,
                deprecated: true,
                docs_url: super::completion::docs_url_at(
                    &bundle.schema,
                    &found.pointer,
                    behavior.repetition() != super::language::Repetition::None,
                ),
                ..Diagnostic::default()
            })
        })
        .collect()
}

/// For an `anyOf` whose every branch wants another type: `true is not of
/// type "integer", "string", "null"`, instead of "not valid under any of the
/// schemas".
#[cfg(feature = "schema-validation")]
fn expected_types(
    error: &ValidationError<'_>,
    branches: &[Vec<ValidationError<'_>>],
    strings: &EditorStrings,
) -> Option<String> {
    let mut types: Vec<String> = Vec::new();
    for nested in branches.iter().flatten() {
        if !matches!(nested.kind(), ValidationErrorKind::Type { .. }) {
            return None;
        }
        for name in listed_types(&nested.to_string())? {
            if !types.contains(&name) {
                types.push(name);
            }
        }
    }
    (!types.is_empty()).then(|| {
        fill(
            &strings.wrong_type,
            &[("value", &error.instance()), ("types", &types.join(", "))],
        )
    })
}

/// The types a validator's type message lists: `"integer"` and `"string"`
/// from `true is not of types "integer", "string"`.
#[cfg(feature = "schema-validation")]
fn listed_types(message: &str) -> Option<Vec<String>> {
    let listed = message
        .split_once(" is not of type")
        .map(|(_, rest)| rest)?;
    let listed = listed.strip_prefix('s').unwrap_or(listed);
    Some(
        listed
            .split(',')
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect(),
    )
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
            [r#"/web: "listen" is not allowed here"#]
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
                .any(|message| message.contains(r#""bogus" is not allowed"#)),
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
mod cap_tests {
    use super::{Diagnostic, MAX_DIAGNOSTICS, MORE_PROBLEMS_CODE, Severity, capped};
    use crate::EditorStrings;

    /// QA 19 (H19-F4): the cap keeps the most serious problems: warnings
    /// listed first no longer crowd the errors out.
    #[test]
    fn the_cap_keeps_errors_over_less_serious_problems_in_their_order() {
        let at = |line: usize, severity: Severity| Diagnostic {
            line: Some(line),
            column: Some(1),
            severity,
            ..Diagnostic::default()
        };
        let mut problems: Vec<Diagnostic> =
            (1..=25).map(|line| at(line, Severity::Warning)).collect();
        problems.extend((26..=35).map(|line| at(line, Severity::Error)));
        let kept = capped(problems, &EditorStrings::default());
        let errors = kept
            .iter()
            .filter(|problem| problem.severity == Severity::Error && problem.line.is_some())
            .count();
        assert_eq!(errors, 10, "{kept:#?}");
        let placed: Vec<usize> = kept.iter().filter_map(|problem| problem.line).collect();
        assert_eq!(placed.len(), MAX_DIAGNOSTICS);
        // In the order they were found.
        assert!(
            placed.windows(2).all(|pair| pair[0] < pair[1]),
            "{placed:?}"
        );
        let note = kept.last().unwrap();
        assert_eq!(note.code.as_deref(), Some(MORE_PROBLEMS_CODE));
        assert_eq!(note.severity, Severity::Warning);
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
            ..Diagnostic::default()
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
    fn a_marker_over_several_lines_moves_its_end_with_them() {
        let over = |line, end_line| Diagnostic {
            end_line: Some(end_line),
            end_column: Some(3),
            ..on(line)
        };
        let spans = |found: Vec<Diagnostic>| -> Vec<_> {
            found
                .into_iter()
                .map(|diagnostic| (diagnostic.line, diagnostic.end_line, diagnostic.end_column))
                .collect()
        };
        // A line inserted inside it: its end moves down.
        let shifted = shift_diagnostics("a\nb\nc\nd", "a\nb\nx\nc\nd", &[over(1, 3)]);
        assert_eq!(spans(shifted), [(Some(1), Some(4), Some(3))]);
        // Above it: all of it moves.
        let shifted = shift_diagnostics("a\nb\nc\nd", "z\na\nb\nc\nd", &[over(2, 4)]);
        assert_eq!(spans(shifted), [(Some(3), Some(5), Some(3))]);
        // Typing on its last line before its end moves the end column only.
        let shifted = shift_diagnostics("abc\ndef", "abc\nXdef", &[over(1, 2)]);
        assert_eq!(spans(shifted), [(Some(1), Some(2), Some(4))]);
        assert_eq!(
            shift_diagnostics("abc\ndef", "abc\nXdef", &[over(1, 2)])[0].column,
            Some(1)
        );
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
        let (took, problems) =
            crate::editor::test_timing::best_of(3, || schema.validate(Language::YAML, &document));
        assert!(took < std::time::Duration::from_millis(300), "{took:?}");
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

#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod placing_tests {
    use super::super::Language;
    use super::Schema;

    /// QA 17: the pause after a key in a 10,000-line TOML or HCL document
    /// with thousands of schema problems took a second: each of the 20
    /// problems shown, and each of its ancestors, was looked up with a read
    /// of the whole document. Their spans are read in one pass now, in
    /// every language, and land where the one-by-one lookups put them.
    #[test]
    fn problems_on_a_large_document_are_placed_in_one_pass_in_every_language() {
        let schema =
            Schema::compile(include_str!("../../tests/fixtures/protected.schema.json")).unwrap();
        let mut clients = serde_json::Map::new();
        for index in 0..2500 {
            clients.insert(
                format!("c{index}"),
                serde_json::json!({"secret": format!("s{index}"), "pin": "not a number"}),
            );
        }
        let document = serde_json::json!({ "clients": clients });
        let mut failures = Vec::new();
        for language in Language::ALL {
            let text = language.serialize(&document).unwrap();
            // The best of five runs: on a loaded machine (other builds, the
            // browser suite) one pass of JSON reached 430 ms; the one-by-one
            // lookups this guards against took 900 ms and more.
            let (took, problems) =
                crate::editor::test_timing::best_of(5, || schema.validate(*language, &text));
            let placed = problems
                .iter()
                .filter(|problem| problem.line.is_some())
                .count();
            if took.as_millis() > 650 || placed < 20 {
                failures.push(format!("{language}: {placed} placed in {took:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "yaml",
    feature = "json"
))]
mod key_placing_tests {
    use super::super::Language;
    use super::super::work_count::{Work, counted};
    use super::Schema;

    /// QA 18: switching a large document to YAML or JSON paused 300-650 ms,
    /// most of it placing the problems shown: each unknown key's marker read
    /// the whole document again (its key lines, or its JSON scan). One read
    /// serves every problem of a pass now.
    #[test]
    fn problems_on_keys_of_a_large_document_read_it_once() {
        let schema = Schema::compile(
            r#"{"type": "object", "properties": {"a": {"type": "object", "additionalProperties": false, "properties": {"k": {}}}}}"#,
        )
        .unwrap();
        let mut a = serde_json::Map::new();
        for index in 0..12000 {
            a.insert(format!("unknown_{index}"), serde_json::json!(index));
        }
        let document = serde_json::json!({ "a": a });
        let mut failures = Vec::new();
        for language in [Language::YAML, Language::JSON] {
            let text = language.serialize(&document).unwrap();
            let (problems, reads) =
                counted(Work::DocumentRead, || schema.validate(language, &text));
            let placed = problems
                .iter()
                .filter(|problem| problem.line.is_some())
                .count();
            // Two reads (the values, the keys) for every problem placed;
            // read per problem, 20 and more.
            if reads > 2 || placed < 20 {
                failures.push(format!("{language}: {placed} placed in {reads} reads"));
            }
            // And the pass itself stays linear: about 30 ms here, where work
            // growing with the square of 12,000 keys takes seconds.
            let (took, _) =
                crate::editor::test_timing::best_of(5, || schema.validate(language, &text));
            if took.as_millis() > 300 {
                failures.push(format!("{language}: validated in {took:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
