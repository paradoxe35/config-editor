//! A field the document gives one of the host's field tokens without the
//! text writing it there: a language's reference copying another field's
//! value (a YAML alias `note: *k` to `key: &k «secret 3f2a»`, a merge key
//! copying a map holding one). Like a token pasted there, it stands for
//! another field's secret, never this one's: flagged on the field, as a
//! moved token is where it is written.

use std::collections::HashMap;

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::ConfigLanguage;
use super::super::paths::{field_key, field_range};
use super::moved_off::is_own_field;
use super::texts;
use super::tokens::{Known, field_value};
use crate::EditorStrings;

/// A warning on each field of `document` (the parse of `text`) holding
/// one of `known`'s field tokens more often than `text` writes it
/// (`written`: how often it is written; `holders`: the fields holding
/// each token, see `SecretMatcher::holders`), that is none of the token's
/// own fields and does not write it.
pub(super) fn copied_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    document: &Value,
    known: &[Known],
    written: &HashMap<&str, usize>,
    holders: &HashMap<String, Vec<String>>,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for (token, pointers) in holders {
        // Only a token held more often than written is copied somewhere.
        if pointers.len() <= written.get(token.as_str()).copied().unwrap_or(0) {
            continue;
        }
        // The token's own fields by what names them alike (a lookup per
        // holder, never a pass over them: one mask in thousands of fields).
        let mut owner = None;
        let mut own: HashMap<String, Vec<(&str, usize)>> = HashMap::new();
        for known in known.iter().filter(|known| known.token == *token) {
            let Some(field) = known.field.as_deref() else {
                continue;
            };
            owner.get_or_insert(field);
            own.entry(field_key(field))
                .or_default()
                .push((field, known.count));
        }
        let Some(owner) = owner else {
            continue;
        };
        for pointer in pointers {
            let slot = own.get_mut(&field_key(pointer)).and_then(|fields| {
                fields.iter_mut().find(|(field, count)| {
                    *count > 0 && is_own_field(Some(document), field, pointer)
                })
            });
            if let Some(slot) = slot {
                slot.1 -= 1;
                continue;
            }
            // Written there: flagged where it is written.
            if writes(behavior, text, document, pointer, token) {
                continue;
            }
            let message = texts::moved_placeholder(strings, pointer, owner);
            problems.push(on_field(behavior, text, pointer, token, message));
        }
    }
    problems.sort_by_key(|problem| (problem.line, problem.column));
    problems
}

/// Whether the field at `pointer` writes `token` in `text`: its value as
/// written holds it.
fn writes(
    behavior: &dyn ConfigLanguage,
    text: &str,
    document: &Value,
    pointer: &str,
    token: &str,
) -> bool {
    if field_value(document, pointer).is_none() {
        return false;
    }
    super::super::paths::written_value_span(behavior, text, pointer)
        .and_then(|span| super::super::codec::scalar_range(behavior, text, span))
        .and_then(|range| text.get(range))
        .is_some_and(|written| written.contains(token))
}

/// A warning `message` on the field at `pointer`, or the nearest field
/// holding it written on a line without `token` (a merge key's copies are
/// read where their source writes them, beside the token).
fn on_field(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    token: &str,
    message: String,
) -> Diagnostic {
    let mut at = pointer;
    loop {
        let range = field_range(behavior, text, at).filter(|range| {
            text.split('\n')
                .nth(range.start.line.saturating_sub(1))
                .is_some_and(|line| !line.contains(token))
        });
        if let Some(range) = range {
            return Diagnostic {
                message,
                severity: Severity::Warning,
                line: Some(range.start.line),
                column: Some(range.start.column),
                end_column: (range.end.line == range.start.line).then_some(range.end.column),
                ..Diagnostic::default()
            };
        }
        match at.rfind('/') {
            Some(cut) if !at.is_empty() => at = &at[..cut],
            _ => break,
        }
    }
    Diagnostic {
        message,
        severity: Severity::Warning,
        ..Diagnostic::default()
    }
}
