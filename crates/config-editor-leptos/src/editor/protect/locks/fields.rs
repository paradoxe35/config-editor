//! Which fields of a text are protected: those the schema marks read-only
//! (in the parsed text, carried from the text before, or read from a text
//! that does not parse), the host's read-only paths, and the secret fields.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::super::super::language::ConfigLanguage;
use super::super::super::paths::lone_spelling;
use super::super::locate::{lookup, plain};
use super::super::scan;
use super::super::texts::Reason;
use super::Protected;
use super::reading::Reading;

/// The fields marked read-only: those of the text before when this one
/// does not parse and carries from it; else, in a text that does not
/// parse, those read from its stand-in and its lines; else those the
/// schema marks in the parsed text (`read_only`, see
/// `SchemaMarks::read_only`).
pub(super) fn marked(
    reading: &Reading<'_>,
    carried: Option<&Protected>,
    read_only: Vec<(String, Option<String>)>,
) -> Vec<(String, Reason)> {
    match (carried, &reading.document, reading.sources.marking_schema) {
        (Some(previous), _, _) => previous.marked.clone(),
        (None, None, Some(schema)) => marked_unparsed(reading, schema),
        _ => read_only
            .into_iter()
            .map(|(pointer, reason)| (pointer, reason.map_or(Reason::Field, Reason::Given)))
            .collect(),
    }
}

/// The fields `schema` marks in a text that does not parse: the stand-in's
/// marks, and those on the lines it blanked; without a stand-in, those on
/// every line.
fn marked_unparsed(reading: &Reading<'_>, schema: &Value) -> Vec<(String, Reason)> {
    let (mut fields, found) = match reading.stand_in() {
        Some(stand) => (
            stand.protected.marked.clone(),
            scan::written(
                reading.behavior,
                reading.text,
                &reading.lines,
                Some(&stand.blanked),
                &[],
            ),
        ),
        None => (Vec::new(), reading.written().to_vec()),
    };
    for (pointer, reason) in scan::marked(reading.behavior, schema, &found) {
        if !fields.iter().any(|(known, _)| *known == pointer) {
            fields.push((pointer, reason));
        }
    }
    fields
}

/// The fields to lock: those `marked`, plus the host's paths written in
/// the text (any, when it does not parse), sorted, a field inside another
/// left out as covered by it.
pub(super) fn outermost(
    reading: &Reading<'_>,
    marked: &[(String, Reason)],
) -> Vec<(String, Reason)> {
    let mut fields = marked.to_vec();
    let mut known: HashSet<String> = fields.iter().map(|(pointer, _)| pointer.clone()).collect();
    for path in reading.sources.paths {
        let pointer = written_spelling(reading, path.to_pointer());
        let written = reading
            .document
            .as_ref()
            .is_none_or(|document| lookup(document, &pointer).is_some());
        if written && known.insert(pointer.clone()) {
            fields.push((pointer, Reason::Field));
        }
    }
    fields.sort();
    outer_only(reading.behavior, &fields)
}

/// A host's path as the parsed text has it: an index on a lone XML element
/// or HCL block (`…/0`, a one-item list) is the field written without it,
/// where the paths' places find it (`paths::lone_spelling`).
fn written_spelling(reading: &Reading<'_>, pointer: String) -> String {
    match &reading.document {
        Some(document) if lookup(document, &pointer).is_none() => {
            lone_spelling(reading.behavior, reading.text, &pointer)
                .filter(|lone| lookup(document, lone).is_some())
                .unwrap_or(pointer)
        }
        _ => pointer,
    }
}

/// `fields` without those inside another one (`locate::within`): each
/// field's ancestors looked up among the others' plain forms, one set for
/// all, not every other field compared with each.
fn outer_only(behavior: &dyn ConfigLanguage, fields: &[(String, Reason)]) -> Vec<(String, Reason)> {
    // Each plain form, with the distinct pointers written so.
    let mut forms: HashMap<String, Vec<&str>> = HashMap::new();
    for (pointer, _) in fields {
        let spellings = forms.entry(plain(behavior, pointer)).or_default();
        if !spellings.contains(&pointer.as_str()) {
            spellings.push(pointer);
        }
    }
    let inside_another = |pointer: &str| {
        let form = plain(behavior, pointer);
        // Another spelling of the same field, or of the whole document.
        if forms.get(&form).is_some_and(|known| known.len() > 1)
            || (!form.is_empty() && forms.contains_key(""))
        {
            return true;
        }
        let mut end = form.len();
        while let Some(cut) = form[..end].rfind('/') {
            if cut > 0 && forms.contains_key(&form[..cut]) {
                return true;
            }
            end = cut;
        }
        false
    };
    fields
        .iter()
        .filter(|(pointer, _)| !inside_another(pointer))
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
