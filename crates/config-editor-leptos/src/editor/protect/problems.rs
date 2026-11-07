//! Problems with secret placeholders: a token the host did not give, in a
//! secret field, stands for no secret; one of the host's tokens outside its
//! own field stands for another field's secret, so it is plain text there.
//! Both are flagged.

use std::ops::Range;

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::{ConfigLanguage, Repetition};
use super::super::paths::path_at;
use super::marks::marks_at;
use super::texts;
use super::tokens::{Known, SecretMatcher};
use crate::EditorStrings;

/// A warning on each placeholder of `text` that is not a real token (one
/// of `tokens`, UTF-16): one of the host's `known` tokens out of its own
/// field anywhere, and a token the host did not give in a field `schema`
/// marks secret.
pub(crate) fn unknown_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    matcher: &SecretMatcher,
    known: &[Known],
    tokens: &[Range<usize>],
    schema: Option<&Value>,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    let repeats = behavior.repetition() != Repetition::None;
    matcher
        .find(text)
        .into_iter()
        .filter_map(|range| {
            let at = text[..range.start].encode_utf16().count();
            if tokens.iter().any(|token| token.start == at) {
                return None;
            }
            let path = path_at(behavior, text, at)?;
            let token = &text[range.clone()];
            let secret =
                || schema.is_some_and(|schema| marks_at(schema, path.segments(), repeats).1);
            let message = match known.iter().find(|known| known.token == token) {
                Some(owner) => {
                    texts::moved_placeholder(strings, &path.to_pointer(), &owner.pointer)
                }
                None if secret() => texts::unknown_placeholder(strings, &path.to_pointer()),
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
