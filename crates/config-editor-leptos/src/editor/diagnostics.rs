use super::byte_order_mark;
use super::data_reading::DataReading;
use super::language::ConfigLanguage;
#[cfg(feature = "schema-validation")]
use jsonschema::error::ValidationError;
#[cfg(feature = "schema-validation")]
use serde_json::Value;

use super::schema::{SchemaBundle, schema_message};
use crate::EditorStrings;
use crate::strings::{default_strings, fill};

#[cfg(feature = "schema-validation")]
mod fields;
mod follow;
mod host_reading;
pub(super) use follow::follow_diagnostics;
pub(super) use host_reading::{ProblemKey, host_problems, with_host_problems};
#[cfg(feature = "schema-validation")]
mod schema_problems;
#[cfg(feature = "schema-validation")]
use fields::Fields;
#[cfg(feature = "schema-validation")]
mod explanation;
#[cfg(feature = "schema-validation")]
mod meta_wording;
#[cfg(feature = "schema-validation")]
mod placeholders;
#[cfg(feature = "schema-validation")]
mod specific;
#[cfg(feature = "schema-validation")]
use specific::{Explanations, own_errors, specific_errors, text_for_typed_option, wrong_type};
#[cfg(feature = "schema-validation")]
mod unevaluated;
#[cfg(feature = "schema-validation")]
mod wording;

/// The most problems of one kind (the schema's, the secret placeholders')
/// shown and underlined; past it, a note says more follow.
pub(super) const MAX_DIAGNOSTICS: usize = 20;

/// Finds the placeholders a pattern matches in a text, by byte range.
pub(super) type PlaceholderPattern = Box<dyn Fn(&str) -> Vec<std::ops::Range<usize>>>;

/// The secret placeholders the problems of a text are found with.
#[derive(Clone, Copy, Default)]
#[cfg_attr(
    not(feature = "schema-validation"),
    expect(dead_code, reason = "only the schema check reads them")
)]
pub(super) struct Placeholders<'p> {
    /// The host's tokens, each with the field whose current secret it
    /// stands for: the schema check accepts it there whatever its schema
    /// says.
    pub(super) accepted: &'p [(String, String)],
    /// What finds the placeholders of any document (this editor's host's
    /// pattern, and each other document's of a documents view): a quoted
    /// value is never cut inside one.
    pub(super) patterns: &'p [PlaceholderPattern],
}

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

/// `diagnostics` (capped) with the note that the check stopped early, so
/// more problems may follow: the note that the list was cut, reworded,
/// or one like it (the footer's count of errors says "+").
fn incomplete(mut diagnostics: Vec<Diagnostic>, strings: &EditorStrings) -> Vec<Diagnostic> {
    diagnostics.retain(|diagnostic| diagnostic.code.as_deref() != Some(MORE_PROBLEMS_CODE));
    diagnostics.push(Diagnostic {
        message: strings.problems_incomplete.clone(),
        severity: Severity::Error,
        code: Some(MORE_PROBLEMS_CODE.to_string()),
        ..Diagnostic::default()
    });
    diagnostics
}

/// Milliseconds after the last keystroke before revalidating, so half-typed
/// states do not flash squiggles.
pub(super) const DIAGNOSTICS_DEBOUNCE_MS: i32 = 250;

/// The validator of a compiled schema, and the validators that say why a
/// document is invalid, built when first asked.
#[cfg(feature = "schema-validation")]
pub(crate) struct Validation {
    validator: jsonschema::Validator,
    /// The schema the validators are built from.
    validated_schema: Value,
    /// The validator that says why a document is invalid (see
    /// [`explanation`]), built for the first one; `None` when it does not
    /// build.
    explainer: std::sync::OnceLock<Option<jsonschema::Validator>>,
    /// The validator that says why when the explaining one does not
    /// build (see [`explanation::bounded`]).
    bounded: std::sync::OnceLock<Option<jsonschema::Validator>>,
}

#[cfg(feature = "schema-validation")]
impl Validation {
    /// The validator of `schema` (its empty references read as the root
    /// already), or why it does not build.
    pub(crate) fn build(schema: &Value) -> Result<Self, String> {
        let validated_schema =
            super::schema::for_validation(schema).unwrap_or_else(|| schema.clone());
        let validator = jsonschema::validator_for(&validated_schema).map_err(|error| {
            if let Some(message) = super::schema_refs::message(error.kind(), schema) {
                return message;
            }
            meta_wording::message(&error)
        })?;
        Ok(Self {
            validator,
            validated_schema,
            explainer: std::sync::OnceLock::new(),
            bounded: std::sync::OnceLock::new(),
        })
    }

    /// Why `value` is invalid: none when it is valid; else the errors of
    /// the explaining validator, each value explained a bounded number of
    /// times (see [`explanation`]), and whether they are all: not when
    /// their budget ran out.
    fn errors<'v>(&'v self, value: &'v Value) -> (Vec<ValidationError<'v>>, bool) {
        if self.validator.is_valid(value) {
            return (Vec::new(), true);
        }
        let explained = self
            .explainer
            .get_or_init(|| explanation::build(&self.validated_schema))
            .as_ref()
            .map(|explainer| explanation::errors(explainer, value));
        if let Some(explained) = explained.filter(|(errors, _)| !errors.is_empty()) {
            return explained;
        }
        // The explaining validator does not build, or read the schema
        // otherwise (a draft read as another): one in the schema's own
        // draft says why, under the same budget; the validator collects
        // every error before it gives the first, so none collects them
        // unbounded.
        self.bounded
            .get_or_init(|| explanation::bounded(&self.validated_schema))
            .as_ref()
            .map_or((Vec::new(), false), |bounded| {
                explanation::errors(bounded, value)
            })
    }
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

    /// Whether the host reported it: its own problem, or the editor's that
    /// the host said too (it then names the host, see `merge`).
    pub(crate) fn reported_by_host(&self) -> bool {
        self.origin == DiagnosticOrigin::Host || self.source.is_some()
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
    byte_order_mark::read(source, |source, _| {
        headless_validate(source, None, language.behavior())
    })
}

/// [`validate`] of a text from outside the editor, its line breaks read as
/// the editor reads them (see `line_breaks`): every position is one in
/// `source`, and a lone CR the language rejects is its problem.
pub(super) fn headless_validate(
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
    validate_keyed(
        source,
        schema,
        schema_error,
        enabled,
        behavior,
        Placeholders {
            accepted,
            patterns: &[],
        },
        strings,
    )
    .0
}

/// [`validate_with`] with the `placeholders` of every document, and the
/// value and keyword of each schema problem found (see [`ProblemKey`]).
pub(super) fn validate_keyed(
    source: &str,
    schema: Option<&SchemaBundle>,
    schema_error: Option<&str>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
    placeholders: Placeholders<'_>,
    strings: &EditorStrings,
) -> (Vec<Diagnostic>, Vec<ProblemKey>) {
    if !enabled {
        return (Vec::new(), Vec::new());
    }
    if source.trim().is_empty() {
        return (Vec::new(), Vec::new());
    }

    let reading =
        match super::data_reading::read(behavior, source, schema.map(|schema| &schema.schema)) {
            Ok(reading) => reading,
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
                    let quote = rest.chars().next().filter(|first| {
                        behavior
                            .config()
                            .quotes
                            .iter()
                            .any(|quote| quote.starts_with(*first))
                    });
                    let quoted = usize::from(quote.is_some());
                    let body: String = rest.chars().skip(quoted).collect();
                    let word = body
                        .chars()
                        .take_while(|character| {
                            character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
                        })
                        .count();
                    let closing = usize::from(quote.is_some() && body.chars().nth(word) == quote);
                    let width = quoted + word + closing;
                    (width > 1).then_some(column + width)
                });
                let problem = Diagnostic {
                    line: error.line.or(fallback.map(|span| span.line)),
                    column: error.column.or(fallback.map(|span| span.start_column)),
                    end_column: word_end
                        .or(error.column.map(|column| column + 1))
                        .or(fallback.map(|span| span.end_column)),
                    message: error.message,
                    severity: Severity::Error,
                    ..Diagnostic::default()
                };
                return (vec![problem], Vec::new());
            }
        };

    if let Some(error) = schema_error {
        let problem = Diagnostic {
            line: None,
            column: None,
            end_column: None,
            message: schema_message(strings, error),
            severity: Severity::Error,
            ..Diagnostic::default()
        };
        return (vec![problem], Vec::new());
    }

    let deprecated = schema
        .map(|schema| deprecation_warnings(&reading, schema, source, behavior, strings))
        .unwrap_or_default();
    // Without `schema-validation` no check could reject a placeholder.
    #[cfg(not(feature = "schema-validation"))]
    let _ = placeholders;
    #[cfg(feature = "schema-validation")]
    if let Some(schema) = schema {
        let read = schema_problems::Reading {
            data: &reading,
            behavior,
        };
        let (found, complete) = schema_problems::found(
            read,
            source,
            (behavior, &reading),
            schema,
            placeholders,
            strings,
        );
        let keys = found.iter().map(|(key, _)| key.clone()).collect();
        let mut diagnostics: Vec<Diagnostic> = found
            .into_iter()
            .map(|(_, diagnostic)| diagnostic)
            .collect();
        // A key two keywords refuse (`additionalProperties` in one place,
        // `unevaluatedProperties` in another) is one problem.
        let mut seen = std::collections::HashSet::new();
        diagnostics.retain(|diagnostic| {
            seen.insert((
                diagnostic.line,
                diagnostic.column,
                diagnostic.message.clone(),
            ))
        });
        diagnostics.extend(deprecated);
        // In the order they are written (the validator finds them in the
        // schema's order); one with no place first.
        diagnostics.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
        let diagnostics = capped(diagnostics, strings);
        let diagnostics = if complete {
            diagnostics
        } else {
            incomplete(diagnostics, strings)
        };
        return (diagnostics, keys);
    }
    (deprecated, Vec::new())
}

/// Pointers of the values of `document` that are one of `accepted`: each
/// string looked up among the tokens, then its field among the token's (a
/// pass over them all for each string took seconds with thousands of
/// placeholders; over a token's fields, with one fixed mask in thousands
/// of fields, the typing pause grew with their square).
#[cfg(feature = "schema-validation")]
fn accepted_pointers(document: &Value, accepted: &[(String, String)]) -> Vec<String> {
    if accepted.is_empty() {
        return Vec::new();
    }
    super::work_count::note(super::work_count::Work::TokenScan);
    let mut fields: std::collections::HashMap<&str, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for (token, field) in accepted {
        fields
            .entry(token.as_str())
            .or_default()
            .insert(super::paths::field_key(field));
    }
    let mut found = Vec::new();
    let mut stack = vec![(String::new(), document)];
    while let Some((pointer, value)) = stack.pop() {
        match value {
            Value::String(text)
                if fields.get(text.as_str()).is_some_and(|fields| {
                    super::work_count::note(super::work_count::Work::PlaceLookup);
                    fields.contains(&super::paths::field_key(&pointer))
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
    reading: &DataReading,
    bundle: &SchemaBundle,
    source: &str,
    behavior: &dyn ConfigLanguage,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if !bundle.has_deprecated {
        return Vec::new();
    }
    let found = super::completion::deprecated_keys(
        reading.value(),
        &bundle.schema,
        MAX_DEPRECATION_WARNINGS,
    );
    // Every key's place read in one pass over the text, not one each.
    let pointers: Vec<String> = found.iter().map(|found| found.pointer.clone()).collect();
    super::paths::with_spans(behavior, source, &pointers, || {
        deprecation_diagnostics(found, source, reading, behavior, strings)
    })
}

/// The warnings of the deprecated keys `found`, each on its key.
fn deprecation_diagnostics(
    found: Vec<super::completion::DeprecatedKey>,
    source: &str,
    reading: &DataReading,
    behavior: &dyn ConfigLanguage,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    found
        .into_iter()
        .filter_map(|found| {
            // The key alone, struck through as VS Code does.
            let span =
                super::paths::schema_problem_key_span(behavior, source, reading, &found.pointer)?;
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
                docs_url: found.docs_url,
                ..Diagnostic::default()
            })
        })
        .collect()
}

#[cfg(feature = "schema-validation")]
fn escape_pointer(segment: &str) -> String {
    crate::editor::language::pointer_key(segment)
}

#[cfg(all(test, feature = "schema-validation", feature = "yaml"))]
mod message_tests {
    use super::super::Language;
    use crate::editor::schema::Schema;

    fn messages(yaml: &str) -> Vec<String> {
        Schema::compile(include_str!("../../tests/fixtures/tunnel.schema.json"))
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

    /// the cap keeps the most serious problems: warnings
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
mod follow_tests {
    use super::super::edit::Change;
    use super::{Diagnostic, Severity, follow_diagnostics};

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

    /// `diagnostics` of `checked` through `range` of it replaced by
    /// `inserted`.
    fn follow(
        checked: &str,
        range: std::ops::Range<usize>,
        inserted: &str,
        diagnostics: &[Diagnostic],
    ) -> Vec<Diagnostic> {
        let change: Change =
            Change::replacing(checked.len(), [(range, inserted.to_string())]).expect("fits");
        let current = change.apply(checked).expect("made for it");
        follow_diagnostics(&change, checked, &current, diagnostics)
    }

    #[test]
    fn markers_below_an_inserted_line_move_down() {
        let followed = follow("a\nb\nc", 2..2, "\n", &[on(1), on(3)]);
        assert_eq!(lines(followed), [Some(1), Some(4)]);
    }

    #[test]
    fn markers_below_a_removed_line_move_up_and_the_removed_ones_go() {
        let followed = follow("a\nb\nc\nd", 2..6, "", &[on(2), on(4)]);
        assert_eq!(lines(followed), [Some(2)]);
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
        let followed = follow("a\nb\nc\nd", 4..4, "x\n", &[over(1, 3)]);
        assert_eq!(spans(followed), [(Some(1), Some(4), Some(3))]);
        // Above it: all of it moves.
        let followed = follow("a\nb\nc\nd", 0..0, "z\n", &[over(2, 4)]);
        assert_eq!(spans(followed), [(Some(3), Some(5), Some(3))]);
        // Typing on its last line before its end moves the end column only.
        let followed = follow("abc\ndef", 4..4, "X", &[over(1, 2)]);
        assert_eq!(followed[0].column, Some(1));
        assert_eq!(spans(followed), [(Some(1), Some(2), Some(4))]);
    }

    #[test]
    fn typing_within_a_line_moves_nothing_on_other_lines() {
        let followed = follow("a: 1\nb: x", 4..4, "2", &[on(2)]);
        assert_eq!(lines(followed), [Some(2)]);
    }

    #[test]
    fn typing_before_a_marker_on_its_line_moves_its_columns() {
        let mut marker = on(1);
        marker.column = Some(9);
        marker.end_column = Some(10);
        let followed = follow("limits: {max: 0}", 7..7, "  ", &[marker]);
        assert_eq!(
            (followed[0].column, followed[0].end_column),
            (Some(11), Some(12))
        );
    }

    /// Text written right at a marker's ends stays outside it; text
    /// written over all of it takes it away until the next check.
    #[test]
    fn a_marker_keeps_to_its_own_text() {
        let mut marker = on(1);
        marker.column = Some(4);
        marker.end_column = Some(7);
        let marked = |found: &[Diagnostic]| -> Vec<_> {
            found
                .iter()
                .map(|diagnostic| (diagnostic.column, diagnostic.end_column))
                .collect()
        };
        // `a: bad`: typed before and after it.
        let before = follow("a: bad", 3..3, "x", std::slice::from_ref(&marker));
        assert_eq!(marked(&before), [(Some(5), Some(8))]);
        let after = follow("a: bad", 6..6, "x", std::slice::from_ref(&marker));
        assert_eq!(marked(&after), [(Some(4), Some(7))]);
        // Written over: gone.
        assert!(follow("a: bad", 3..6, "good", &[marker]).is_empty());
    }
}

#[cfg(all(test, feature = "schema-validation", feature = "yaml"))]
mod scale_tests {
    use super::super::Language;
    use crate::editor::schema::Schema;

    #[test]
    fn many_unknown_keys_stay_fast_and_messages_stay_short() {
        let schema =
            Schema::compile(include_str!("../../tests/fixtures/tunnel.schema.json")).unwrap();
        let document = |keys: usize| -> String {
            (0..keys)
                .map(|index| format!("unknown_{index}: 1\n"))
                .collect()
        };
        let (small, large) = (document(500), document(2000));
        // The problems placed with a few reads of the document, not one each.
        let (problems, reads) = crate::editor::work_count::whole_reads(&large, || {
            schema.validate(Language::YAML, &large)
        });
        assert!(reads <= 8, "{reads} whole reads");
        assert!(problems.iter().all(|problem| problem.message.len() < 200));
        // And the validator's own time (not counted) grows with the keys:
        // four times the keys about four times as long.
        let growth = crate::editor::test_timing::growth(5, (&small, &large), |text: &String| {
            schema.validate(Language::YAML, text)
        });
        assert!(growth.ratio() < 10.0, "500 keys, then 2000: {growth}");
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
    use crate::editor::schema::Schema;

    /// the pause after a key in a 10,000-line TOML or HCL document
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
            // Counted: the one-by-one lookups this guards against read the
            // whole document for each problem and each of its ancestors.
            let (problems, reads) =
                crate::editor::work_count::whole_reads(&text, || schema.validate(*language, &text));
            let placed = problems
                .iter()
                .filter(|problem| problem.line.is_some())
                .count();
            if reads > 8 || placed < 20 {
                failures.push(format!(
                    "{language}: {placed} placed in {reads} whole reads"
                ));
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
    use crate::editor::schema::Schema;

    /// switching a large document to YAML or JSON paused 300-650 ms,
    /// most of it placing the problems shown: each unknown key's marker read
    /// the whole document again (its key lines, or its JSON scan). One read
    /// serves every problem of a pass now.
    #[test]
    fn problems_on_keys_of_a_large_document_read_it_once() {
        let schema = Schema::compile(
            r#"{"type": "object", "properties": {"a": {"type": "object", "additionalProperties": false, "properties": {"k": {}}}}}"#,
        )
        .unwrap();
        let document = |keys: usize| {
            let mut a = serde_json::Map::new();
            for index in 0..keys {
                a.insert(format!("unknown_{index}"), serde_json::json!(index));
            }
            serde_json::json!({ "a": a })
        };
        let (small, document) = (document(3000), document(12000));
        let mut failures = Vec::new();
        for language in [Language::YAML, Language::JSON] {
            let text = language.serialize(&document).unwrap();
            let small = language.serialize(&small).unwrap();
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
            // And the pass itself stays linear (timed: most of it is the
            // validator's own): four times the keys about four times as
            // long, sixteen times when it grows with their square.
            let growth = crate::editor::test_timing::growth(5, (&small, &text), |text: &String| {
                schema.validate(language, text)
            });
            if growth.ratio() > 10.0 {
                failures.push(format!("{language}: 3000 keys, then 12000: {growth}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[cfg(all(test, feature = "schema-validation", feature = "json"))]
mod placeholder_tests;

#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod secret_shape_tests;
