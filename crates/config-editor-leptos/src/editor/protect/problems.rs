//! Problems with secret placeholders: a token the host did not give, in a
//! secret field, stands for no secret; one of the host's tokens outside its
//! own field stands for another field's secret, so it is plain text there.
//! Both are flagged.

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::{ConfigLanguage, Repetition};
use super::super::paths::{path_at, same_field};
use super::marks::marks_at;
use super::texts;
use super::tokens::{Known, SecretMatcher, held_in_fields};
use crate::EditorStrings;

/// A warning on each placeholder of `text` that is not real: one of the
/// host's `known` tokens out of its own field anywhere, and a token the host
/// did not give in a field `schema` marks secret.
///
/// Read from the text alone (not the placeholders' places on screen, which
/// move with every key): one parse says whether each known token is held in
/// its own field as often as it is written; only a token that is not, or
/// one the host did not give, is looked up by path.
pub(crate) fn unknown_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    matcher: &SecretMatcher,
    known: &[Known],
    schema: Option<&Value>,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    let found = matcher.find(text);
    if found.is_empty() {
        return Vec::new();
    }
    let repeats = behavior.repetition() != Repetition::None;
    let document = behavior.parse(text).ok();
    // Known tokens the document holds in their fields as often as written.
    let settled = |token: &str| {
        let written = found
            .iter()
            .filter(|range| &text[(*range).clone()] == token)
            .count();
        document
            .as_ref()
            .is_some_and(|document| held_in_fields(document, known, token) == written)
    };
    let mut checked: Vec<(&str, bool)> = Vec::new();
    let mut left: Vec<(&str, String, usize)> = known
        .iter()
        .map(|known| (known.token.as_str(), known.pointer.clone(), known.count))
        .collect();
    found
        .iter()
        .filter_map(|range| {
            let token = &text[range.clone()];
            let owner = known.iter().find(|known| known.token == token);
            if owner.is_some() {
                let quiet = match checked.iter().find(|(seen, _)| *seen == token) {
                    Some((_, quiet)) => *quiet,
                    None => {
                        let quiet = settled(token);
                        checked.push((token, quiet));
                        quiet
                    }
                };
                if quiet {
                    return None;
                }
            } else if schema.is_none() {
                return None;
            }
            let at = text[..range.start].encode_utf16().count();
            let path = path_at(behavior, text, at)?;
            let pointer = path.to_pointer();
            let message = match owner {
                Some(owner) => {
                    // In its own field (as often as the host wrote it there):
                    // real.
                    if let Some(slot) = left.iter_mut().find(|(seen, field, count)| {
                        *seen == token && *count > 0 && same_field(field, &pointer)
                    }) {
                        slot.2 -= 1;
                        return None;
                    }
                    texts::moved_placeholder(strings, &pointer, &owner.pointer)
                }
                None if schema
                    .is_some_and(|schema| marks_at(schema, path.segments(), repeats).1) =>
                {
                    texts::unknown_placeholder(strings, &pointer)
                }
                None => return None,
            };
            let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let column = text[line_start..range.start].chars().count() + 1;
            Some(Diagnostic {
                line: Some(text[..line_start].matches('\n').count() + 1),
                column: Some(column),
                end_column: Some(column + token.chars().count()),
                message,
                severity: Severity::Warning,
                ..Diagnostic::default()
            })
        })
        .collect()
}
