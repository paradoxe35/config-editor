//! Problems with secret placeholders: a token in a secret field the host
//! did not give stands for no secret, so it is flagged.

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::{ConfigLanguage, Repetition};
use super::super::paths::path_at;
use super::marks::marks_at;
use super::texts;
use super::tokens::SecretMatcher;

/// A warning on each placeholder in a secret field of `text` that is not
/// one of the host's `known` tokens.
pub(crate) fn unknown_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    matcher: &SecretMatcher,
    known: &[(String, usize)],
    schema: &Value,
) -> Vec<Diagnostic> {
    let repeats = behavior.repetition() != Repetition::None;
    matcher
        .find(text)
        .into_iter()
        .filter(|range| !known.iter().any(|(token, _)| *token == text[range.clone()]))
        .filter_map(|range| {
            let at = text[..range.start].encode_utf16().count();
            let path = path_at(behavior, text, at)?;
            if !marks_at(schema, path.segments(), repeats).1 {
                return None;
            }
            let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let column = text[line_start..range.start].chars().count() + 1;
            Some(Diagnostic {
                line: Some(text[..line_start].matches('\n').count() + 1),
                column: Some(column),
                end_column: Some(column + text[range].chars().count()),
                message: texts::unknown_placeholder(&path.to_pointer()),
                severity: Severity::Warning,
                ..Diagnostic::default()
            })
        })
        .collect()
}
