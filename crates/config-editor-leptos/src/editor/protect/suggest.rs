//! Suggestions around protected fields: read-only keys and values are not
//! offered (inside a read-only region, where browsing shows them, their
//! detail gives the reason).

use serde_json::Value;

use super::super::completion::{Completion, path_with_key};
use super::super::language::{ConfigLanguage, Repetition, SchemaPathSegment};
use super::super::paths::FieldPath;
use super::locate::within;
use super::locks::Protected;
use super::marks::marks_at;
use super::texts;

/// What the suggestions are adjusted for.
pub(crate) struct Around<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) source: &'a str,
    pub(crate) cursor: usize,
    pub(crate) schema: Option<&'a Value>,
    pub(crate) host_paths: &'a [FieldPath],
    pub(crate) protected: &'a Protected,
}

/// `options` without what a protected field rules out.
pub(crate) fn adjust(options: Vec<Completion>, around: &Around<'_>) -> Vec<Completion> {
    let behavior = around.behavior;
    if behavior
        .lexical_state(around.source, around.cursor)
        .in_comment
    {
        return options;
    }
    let context = behavior.completion_context(around.source, around.cursor);
    let browsing = around.protected.lock_at(around.cursor).is_some();
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
        return Some(reason.unwrap_or_else(|| texts::DEFAULT_REASON.to_string()));
    }
    let pointer = FieldPath::from(path.to_vec()).to_pointer();
    around
        .host_paths
        .iter()
        .any(|locked| within(around.behavior, &pointer, &locked.to_pointer()))
        .then(|| texts::DEFAULT_REASON.to_string())
}
