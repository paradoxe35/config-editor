//! Which fields of a text are protected: those the schema marks read-only
//! (in the parsed text, carried from the text before, or read from a text
//! that does not parse), the host's read-only paths, and the secret fields.

use serde_json::Value;

use super::super::super::language::ConfigLanguage;
use super::super::locate::{lookup, within};
use super::super::scan;
use super::super::texts::Reason;
use super::Protected;
use super::reading::Reading;

/// The fields the schema marks read-only, with their reasons.
pub(super) struct Marked {
    pub(super) fields: Vec<(String, Reason)>,
    /// The text does not parse, has no stand-in and is too large to read
    /// line by line: what it holds is not known, so it is held as a field
    /// not found.
    pub(super) unread: bool,
}

/// The fields marked read-only: those of the text before when this one
/// does not parse and carries from it; else, in a text that does not
/// parse, those read from its stand-in and its lines; else those the
/// schema marks in the parsed text (`read_only`, see
/// `SchemaMarks::read_only`).
pub(super) fn marked(
    reading: &Reading<'_>,
    carried: Option<&Protected>,
    read_only: Vec<(String, Option<String>)>,
) -> Marked {
    let fields = match (carried, &reading.document, reading.sources.marking_schema) {
        (Some(previous), _, _) => previous.marked.clone(),
        (None, None, Some(schema)) => return marked_unparsed(reading, schema),
        _ => read_only
            .into_iter()
            .map(|(pointer, reason)| (pointer, reason.map_or(Reason::Field, Reason::Given)))
            .collect(),
    };
    Marked {
        fields,
        unread: false,
    }
}

/// The fields `schema` marks in a text that does not parse: the stand-in's
/// marks, and those on the lines it blanked; without a stand-in, those on
/// every line.
fn marked_unparsed(reading: &Reading<'_>, schema: &Value) -> Marked {
    let (mut fields, found) = match reading.stand_in() {
        Some(stand) => (
            stand.protected.marked.clone(),
            scan::written(
                reading.behavior,
                reading.text,
                &reading.lines,
                Some(&stand.blanked),
            ),
        ),
        None => (Vec::new(), reading.written().map(<[_]>::to_vec)),
    };
    let unread = found.is_none();
    for (pointer, reason) in scan::marked(reading.behavior, schema, &found.unwrap_or_default()) {
        if !fields.iter().any(|(known, _)| *known == pointer) {
            fields.push((pointer, reason));
        }
    }
    Marked { fields, unread }
}

/// The fields to lock: those `marked`, plus the host's paths written in
/// the text (any, when it does not parse), sorted, a field inside another
/// left out as covered by it.
pub(super) fn outermost(
    reading: &Reading<'_>,
    marked: &[(String, Reason)],
) -> Vec<(String, Reason)> {
    let mut fields = marked.to_vec();
    for path in reading.sources.paths {
        let pointer = path.to_pointer();
        let written = reading
            .document
            .as_ref()
            .is_none_or(|document| lookup(document, &pointer).is_some());
        if written && !fields.iter().any(|(known, _)| *known == pointer) {
            fields.push((pointer, Reason::Field));
        }
    }
    fields.sort();
    outer_only(reading.behavior, &fields)
}

/// `fields` without those inside another one.
fn outer_only(behavior: &dyn ConfigLanguage, fields: &[(String, Reason)]) -> Vec<(String, Reason)> {
    fields
        .iter()
        .filter(|(pointer, _)| {
            !fields
                .iter()
                .any(|(other, _)| other != pointer && within(behavior, pointer, other))
        })
        .cloned()
        .collect()
}

/// The secret fields: those of the text before when this one does not
/// parse and carries from it; else, in a text that does not parse, the
/// stand-in's; else those the schema marks in the parsed text (`marked`,
/// see `SchemaMarks::secret`).
pub(super) fn secret(
    reading: &Reading<'_>,
    carried: Option<&Protected>,
    marked: Vec<String>,
) -> Vec<String> {
    match (carried, &reading.document) {
        (Some(previous), _) => previous.secret.clone(),
        (None, None) => reading
            .stand_in()
            .map(|stand| stand.protected.secret.clone())
            .unwrap_or_default(),
        (None, Some(_)) => marked,
    }
}
