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
}

/// `options` without what a protected field rules out, or the reference
/// forms in a secret field's value.
pub(crate) fn adjust(options: Vec<Completion>, around: &Around<'_>) -> Vec<Completion> {
    let behavior = around.behavior;
    if behavior
        .lexical_state(around.source, around.cursor)
        .in_comment
    {
        return options;
    }
    let context = behavior.completion_context(around.source, around.cursor);
    let repeats = behavior.repetition() != Repetition::None;
    if let Some(key) = context.value_key.as_deref() {
        let path = path_with_key(&context.path, key);
        let secret = around
            .schema
            .is_some_and(|schema| marks_at(schema, &path, repeats).1);
        if secret && lock_reason(around, &path).is_none() {
            let typed = unquoted(&context.typed);
            if !around.explicit && !typed.is_empty() && !options.is_empty() {
                return options;
            }
            let leading = if context.value_needs_space { " " } else { "" };
            return references(
                around,
                leading,
                typed,
                context.replace_start..context.replace_end,
            );
        }
    }
    // Browsing a read-only region: on its text, or on a new line inside a
    // read-only block (whose keys would all be read-only).
    let browsing = around.protected.lock_at(around.cursor).is_some()
        || (!context.path.is_empty() && lock_reason(around, &context.path).is_some());
    options
        .into_iter()
        .filter_map(|mut option| {
            let path = match context.value_key.as_deref() {
                Some(key) if option.is_value => path_with_key(&context.path, key),
                _ if option.is_value => return Some(option),
                _ => path_with_key(&context.path, &option.label),
            };
            match lock_reason(around, &path) {
                None => Some(option),
                Some(reason) if browsing => {
                    option.detail = reason;
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

/// The reference forms, written as the language writes a string value, the
/// caret just after the prefix (inside `${…}`).
fn references(
    around: &Around<'_>,
    leading: &str,
    typed: &str,
    replaced: std::ops::Range<usize>,
) -> Vec<Completion> {
    around
        .prefixes
        .iter()
        .filter(|prefix| !prefix.is_empty())
        .filter_map(|prefix| {
            let closer = if prefix.ends_with('{') { "}" } else { "" };
            let label = texts::reference_label(prefix, closer);
            if !typed.is_empty() && match_score(&label, typed).is_none() {
                return None;
            }
            let written = around
                .behavior
                .format_value(&Value::String(format!("{prefix}{closer}")));
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
                cursor_offset: format!("{leading}{}", &written[..caret])
                    .encode_utf16()
                    .count(),
                insert,
                start: replaced.start,
                end: replaced.end,
                selected: 0,
                is_value: true,
                renames: false,
                deprecated: false,
                fills: Vec::new(),
                closes_tag: false,
                label,
            })
        })
        .collect()
}
