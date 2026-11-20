//! The schema problems of a document's value, placed in a text: its own
//! (the value read from that text), or a host's read in its own language
//! and placed in its Edit-as view (see `host_reading`).

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

use super::secrets::Secrets;
use super::specific::{Explanations, specific_errors};
use super::{
    Diagnostic, Fields, MAX_DIAGNOSTICS, Placeholders, ProblemKey, SchemaBundle, Severity,
    accepted_pointers, cut, escape_pointer, held_texts, placeholders, unevaluated, wording,
};
use crate::EditorStrings;
use crate::editor::data_reading::DataReading;
use crate::editor::language::{ConfigLanguage, Span};
use crate::strings::fill;

/// A document read as data and the language it was read in (how its
/// lists are written, its text key).
#[derive(Clone, Copy)]
pub(super) struct Reading<'r> {
    pub(super) data: &'r DataReading,
    pub(super) behavior: &'r dyn ConfigLanguage,
}

/// The schema problems of `reading`'s value (enough for the cap), each
/// with its [`ProblemKey`] and placed where `placing` finds its value in
/// `source` (read as data `placed`, whose pointers are the problems'); and
/// whether they are all (not when the budget of the explanations ran out).
pub(super) fn found(
    reading: Reading<'_>,
    source: &str,
    (placing, placed): (&dyn ConfigLanguage, &DataReading),
    schema: &SchemaBundle,
    tokens: Placeholders<'_>,
    strings: &EditorStrings,
) -> (Vec<(ProblemKey, Diagnostic)>, bool) {
    let document = reading.data.value();
    let placeholders = Fields::new(accepted_pointers(document, tokens.accepted));
    // The strings holding a placeholder are hidden as a secret's are.
    let secrets = secrets(reading, schema).hiding(held_texts(source, placing, tokens));
    // Infinities and NaN are numbers to the schema (see `non_finite`).
    use crate::editor::codec::non_finite::{StandIn, for_validation};
    let validated = for_validation(document, StandIn::Bounds);
    let restore = matches!(validated, std::borrow::Cow::Owned(_));
    // No one stand-in fails every check an infinity or NaN fails: the
    // other passes add the checks they stand for.
    let further: Vec<(StandIn, std::borrow::Cow<'_, Value>)> = if restore {
        StandIn::ALL[1..]
            .iter()
            .map(|pass| (*pass, for_validation(document, *pass)))
            .collect()
    } else {
        Vec::new()
    };
    // The errors shown (enough for the cap), then where each is
    // written: the spans of their values read at once (see
    // `paths::with_spans`), not a read of the document each.
    let (main, mut complete) = schema.validation.errors(&validated);
    let further: Vec<(StandIn, Vec<ValidationError<'_>>)> = further
        .iter()
        .map(|(pass, value)| {
            let (errors, all) = schema.validation.errors(value);
            complete &= all;
            (*pass, errors)
        })
        .collect();
    // Each option's first explanation, in each validation.
    let explanations = Explanations::new(
        std::iter::once(main.as_slice()).chain(further.iter().map(|(_, errors)| errors.as_slice())),
    );
    // A problem judging a placeholder's unknown value is none.
    let judged = |error: &ValidationError<'_>| {
        placeholders::judges_placeholder(error, &placeholders, &schema.schema, &explanations)
    };
    let mut tops: Vec<&ValidationError<'_>> = Vec::new();
    let mut shown = 0;
    for top in &main {
        shown += specific_errors(&explanations, top)
            .iter()
            .filter(|error| !judged(error))
            .count();
        tops.push(top);
        if shown > MAX_DIAGNOSTICS {
            break;
        }
    }
    let mut found: std::collections::HashSet<(String, String)> = tops
        .iter()
        .flat_map(|top| specific_errors(&explanations, top))
        .map(|error| {
            let keyword = error.kind().keyword().to_string();
            (error.instance_path().as_str().to_string(), keyword)
        })
        .collect();
    for (pass, errors) in &further {
        for error in errors {
            let key = (
                error.instance_path().as_str().to_string(),
                error.kind().keyword().to_string(),
            );
            if shown <= MAX_DIAGNOSTICS
                && pass.reports(error.instance(), error.kind())
                && !judged(error)
                && found.insert(key)
            {
                shown += 1;
                tops.push(error);
            }
        }
    }
    let pointers: Vec<String> = tops
        .iter()
        .flat_map(|top| specific_errors(&explanations, top))
        .map(|error| error.instance_path().as_str().to_string())
        .collect();
    let repeats_arrays = reading.behavior.repetition() != crate::editor::language::Repetition::None;
    let unevaluated = unevaluated::Unevaluated::new(
        &schema.schema,
        &validated,
        repeats_arrays,
        tops.iter()
            .flat_map(|top| specific_errors(&explanations, top)),
    );
    let wording = wording::Wording {
        root: &schema.schema,
        strings,
        secrets: &secrets,
        placeholders: &placeholders,
        tokens: &cut::Tokens::new(
            tokens.accepted.iter().map(|(token, _)| token.as_str()),
            tokens.patterns,
        ),
        levels: std::cell::Cell::new(0),
        explanations: &explanations,
    };
    let found = crate::editor::paths::with_spans(placing, source, &pointers, || {
        tops.iter()
            .flat_map(|top| {
                specific_errors(&explanations, top)
                    .into_iter()
                    .flat_map(|error| {
                        let path = error.instance_path().as_str().to_string();
                        let key = (path.clone(), error.kind().keyword().to_string());
                        if judged(error) {
                            return Vec::new();
                        }
                        // The schema's docs for the value (`x-docs-url`):
                        // its problem links to them, as its hover does.
                        let docs = crate::editor::completion::docs_url_at(
                            &schema.schema,
                            &path,
                            repeats_arrays,
                        );
                        let message = wording.message(error);
                        let message = if restore {
                            crate::editor::codec::non_finite::restore_in_message(&message)
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
                        // Each item a list refuses gets its own marker
                        // where it is written, as an item `items: false`
                        // refuses does.
                        let refused_items = match error.kind() {
                            ValidationErrorKind::AdditionalItems { limit } => Some(
                                (*limit..error.instance().as_array().map_or(0, Vec::len)).collect(),
                            ),
                            ValidationErrorKind::UnevaluatedItems { unexpected } => {
                                Some(unevaluated.refused_items(&path, unexpected))
                            }
                            _ => None,
                        };
                        let locations: Vec<(Option<Span>, String)> = if let Some(indices) =
                            refused_items
                        {
                            indices
                                .into_iter()
                                .take(MAX_DIAGNOSTICS + 1)
                                .filter_map(|index| {
                                    let item = error.instance().as_array()?.get(index)?.clone();
                                    let pointer = format!("{path}/{index}");
                                    let block = item.as_object().is_some_and(|map| !map.is_empty())
                                        || item.as_array().is_some_and(|items| !items.is_empty());
                                    let span = crate::editor::paths::schema_problem_span(
                                        placing, source, placed, &pointer, false, block,
                                    );
                                    let value = wording.value_at(&pointer, &item);
                                    let message = format!(
                                        "{pointer}: {}",
                                        crate::editor::schema::false_schema_message(&value)
                                    );
                                    Some((span, message))
                                })
                                .collect()
                        } else if let Some(unexpected) = match error.kind() {
                            ValidationErrorKind::AdditionalProperties { unexpected } => {
                                Some(unexpected.iter().collect::<Vec<_>>())
                            }
                            ValidationErrorKind::UnevaluatedProperties { unexpected } => {
                                Some(unevaluated.refused_keys(&path, unexpected))
                            }
                            _ => None,
                        } {
                            let parent = error.instance_path().as_str();
                            // Only as many as can be shown are located: each is a scan.
                            unexpected
                                .into_iter()
                                .take(MAX_DIAGNOSTICS + 1)
                                .map(|key| {
                                    let pointer = format!("{parent}/{}", escape_pointer(key));
                                    let span = crate::editor::paths::schema_problem_key_span(
                                        placing, source, placed, &pointer,
                                    );
                                    let message =
                                        if reading.behavior.text_key() == Some(key.as_str()) {
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
                            let location = crate::editor::paths::schema_problem_span(
                                placing,
                                source,
                                placed,
                                &path,
                                match error.kind() {
                                    ValidationErrorKind::Required { .. } => true,
                                    // Which keys the object has decides it,
                                    // unless no option takes an object.
                                    ValidationErrorKind::AnyOf { .. }
                                    | ValidationErrorKind::OneOfNotValid { .. }
                                    | ValidationErrorKind::OneOfMultipleValid { .. } => {
                                        error.instance().is_object()
                                            && !wording.every_option_wants_another_type(error)
                                    }
                                    _ => false,
                                },
                                written_as_block,
                            );
                            vec![(location, message)]
                        };
                        locations
                            .into_iter()
                            .map(|(location, message)| {
                                let location = location.or_else(|| placing.first_span(source));
                                let diagnostic = Diagnostic {
                                    line: location.map(|location| location.line),
                                    column: location.map(|location| location.start_column),
                                    end_column: location.map(|location| location.end_column),
                                    message,
                                    severity: Severity::Error,
                                    docs_url: docs.clone(),
                                    ..Diagnostic::default()
                                };
                                (key.clone(), diagnostic)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .take(MAX_DIAGNOSTICS + 1)
            .collect::<Vec<_>>()
    });
    (found, complete)
}

/// The secrets of `reading`'s value the schema marks (see [`Secrets`]).
/// Where the reading made a value written once a list of one, the texts
/// of the value with each such list read as its item are hidden too: a
/// field one option reads as a list (so the reading made it one) is a
/// secret's under another option, and its text is hidden wherever a
/// message quotes it.
pub(super) fn secrets<'s>(reading: Reading<'s>, schema: &'s SchemaBundle) -> Secrets<'s> {
    if !schema.has_marks {
        return Secrets::none();
    }
    let repeats_arrays = reading.behavior.repetition() != crate::editor::language::Repetition::None;
    let marked = |document: &Value| {
        crate::editor::protect::secret_pointers(document, &schema.schema, repeats_arrays)
    };
    let document = reading.data.value();
    let mut readings = vec![(document, marked(document))];
    let written = reading
        .data
        .has_lists_of_one()
        .then(|| reading.data.as_parsed())
        .flatten();
    if let Some(written) = &written {
        readings.push((written, marked(written)));
    }
    Secrets::new(document, &schema.schema, repeats_arrays, &readings)
}
