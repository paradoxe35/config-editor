//! Suggestions around protected fields: read-only keys and values are not
//! offered (inside a read-only region, where browsing shows them, their
//! detail gives the reason), and a secret field's value offers reference
//! forms (`${VAR}`, `file:`) instead of values.

use serde_json::Value;

use super::super::completion::{Completion, match_ranges, match_score, path_with_key, unquoted};
use super::super::language::{ConfigLanguage, Repetition, SchemaPathSegment};
use super::super::paths::FieldPath;
use super::locate::within;
use super::locks::Protected;
use super::marks::marks_at;
use super::texts;
use crate::EditorStrings;
use crate::editor::text::{TextRange, TextSize, byte_of, text_size, utf16_of};

/// What the suggestions are adjusted for.
pub(crate) struct Around<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) source: &'a str,
    pub(crate) cursor: usize,
    pub(crate) explicit: bool,
    pub(crate) schema: Option<&'a Value>,
    pub(crate) host_paths: &'a [FieldPath],
    pub(crate) protected: &'a Protected,
    pub(crate) prefixes: &'a [String],
    pub(crate) strings: &'a EditorStrings,
    /// The real placeholders of `source` (bytes): a value that is one
    /// is replaced whole by a reference form, as an empty one is filled.
    pub(crate) tokens: &'a [TextRange],
}

/// `options` without what a protected field rules out, or the reference
/// forms in a secret field's value.
pub(crate) fn adjust(options: Vec<Completion>, around: &Around<'_>) -> Vec<Completion> {
    // Nothing to leave out, and without a schema no secret field to offer
    // reference forms in.
    if options.is_empty() && around.schema.is_none() {
        return options;
    }
    let behavior = around.behavior;
    if super::super::parse_cache::lexical_state(behavior, around.source, around.cursor).in_comment {
        return options;
    }
    // At a placeholder's start, the field is read where the placeholder
    // ends: before it, a language may read no value yet (`<k>|«…»`).
    let caret = byte_of(around.source, around.cursor);
    let context_at = around
        .tokens
        .iter()
        .find(|token| token.start() == caret)
        .map_or(around.cursor, |token| utf16_of(around.source, token.end()));
    let context =
        super::super::parse_cache::completion_context(behavior, around.source, context_at);
    let repeats = behavior.repetition() != Repetition::None;
    if let Some(key) = context.value_key.as_deref() {
        let path = path_with_key(&context.path, key);
        let secret = around
            .schema
            .is_some_and(|schema| marks_at(schema, &path, repeats).1);
        if secret && lock_reason(around, &path).is_none() {
            let leading = if context.value_needs_space { " " } else { "" };
            // The language API places the range in UTF-16 units.
            let start = byte_of(around.source, context.replace_start);
            let end = byte_of(around.source, context.replace_end).max(start);
            // A placeholder at the caret (which snaps to its edges) is the
            // secret kept, not text typed: the forms are offered as for an
            // empty value, each replacing it whole.
            let token = around.tokens.iter().find(|token| {
                token.contains_inclusive(caret)
                    || TextRange::new(start, end).contains_range(**token)
            });
            let replaced = token.map_or(TextRange::new(start, end), |token| {
                TextRange::new(start, end).cover(*token)
            });
            let typed = if token.is_some() {
                ""
            } else {
                unquoted(behavior, &context.typed)
            };
            if !around.explicit && !typed.is_empty() && !options.is_empty() {
                return options;
            }
            return references(around, leading, typed, replaced);
        }
    }
    // Browsing a read-only region: on its text, or on a new line inside a
    // read-only block (whose keys would all be read-only).
    // The suggestion API's cursor is UTF-16; the locks count bytes.
    let cursor = crate::editor::text::byte_of(around.source, around.cursor);
    let browsing = around.protected.lock_at(cursor).is_some()
        || (!context.path.is_empty() && lock_reason(around, &context.path).is_some());
    // Every value is the same field's: whether it is read-only is asked once.
    let value_lock = context
        .value_key
        .as_deref()
        .map(|key| lock_reason(around, &path_with_key(&context.path, key)));
    options
        .into_iter()
        .filter_map(|mut option| {
            let lock = match &value_lock {
                Some(lock) if option.is_value => lock.clone(),
                _ if option.is_value => return Some(option),
                _ => lock_reason(around, &path_with_key(&context.path, &option.label)),
            };
            match lock {
                None => Some(option),
                Some(reason) if browsing => {
                    option.detail = reason;
                    option.read_only = true;
                    Some(option)
                }
                Some(_) => None,
            }
        })
        .collect()
}

/// Why the field at `path` is read-only, if it is: the schema's reason, or
/// the host named it (or a field holding it).
fn lock_reason(around: &Around<'_>, path: &[SchemaPathSegment]) -> Option<String> {
    let repeats = around.behavior.repetition() != Repetition::None;
    if let Some(schema) = around.schema
        && let (Some(reason), _) = marks_at(schema, path, repeats)
    {
        return Some(reason.unwrap_or_else(|| around.strings.read_only_field.clone()));
    }
    let pointer = FieldPath::from(path.to_vec()).to_pointer();
    around
        .host_paths
        .iter()
        .any(|locked| within(around.behavior, &pointer, &locked.to_pointer()))
        .then(|| around.strings.read_only_field.clone())
}

/// What closes a reference the configured `prefix` opens: the bracket
/// pairing the one it ends with (`${` → `}`, `$(` → `)`, `{{` → `}`), or
/// nothing (`file:`). Reference forms are the host's, not a language's.
fn closer_of(prefix: &str) -> String {
    const PAIRS: [(char, char); 6] = [
        ('(', ')'),
        ('[', ']'),
        ('{', '}'),
        ('<', '>'),
        ('\u{ab}', '\u{bb}'),
        ('\u{27e8}', '\u{27e9}'),
    ];
    prefix
        .chars()
        .next_back()
        .and_then(|last| PAIRS.iter().find(|(open, _)| *open == last))
        .map(|(_, close)| close.to_string())
        .unwrap_or_default()
}

/// The reference forms, written as the language writes a string value, the
/// caret just after the prefix (inside `${…}`).
fn references(
    around: &Around<'_>,
    leading: &str,
    typed: &str,
    replaced: TextRange,
) -> Vec<Completion> {
    around
        .prefixes
        .iter()
        .filter(|prefix| !prefix.is_empty())
        .filter_map(|prefix| {
            let closer = closer_of(prefix);
            let label = texts::reference_label(prefix, &closer);
            if !typed.is_empty() && match_score(&label, typed).is_none() {
                return None;
            }
            let written = around
                .behavior
                .write_value(&Value::String(format!("{prefix}{closer}")));
            // `${` may be written escaped (`$${` in HCL): the caret goes
            // after the prefix as written.
            let caret = written
                .find(prefix.as_str())
                .map_or(written.len(), |at| at + prefix.len());
            let insert = format!("{leading}{written}");
            Some(Completion {
                matched: match_ranges(&label, typed),
                detail: texts::reference_detail(around.strings, prefix),
                type_label: around.strings.reference_type.clone(),
                cursor_offset: text_size(leading.len() + caret),
                insert,
                start: replaced.start(),
                end: replaced.end(),
                selected: TextSize::default(),
                is_value: true,
                renames: false,
                deprecated: false,
                fills: Vec::new(),
                closes_tag: false,
                key_origin: None,
                read_only: false,
                label,
            })
        })
        .collect()
}

#[cfg(test)]
mod closer_tests {
    #[test]
    fn a_reference_closes_the_bracket_its_prefix_opens() {
        let closers: Vec<String> = ["${", "$(", "{{", "%[", "file:", "vault:"]
            .into_iter()
            .map(super::closer_of)
            .collect();
        assert_eq!(closers, ["}", ")", "}", "]", "", ""]);
    }
}
