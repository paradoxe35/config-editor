//! Completion against the demo's tunnel schema, for the suggestion cases:
//! what is offered at a caret, and the document after accepting a label.
//! Each takes a text marked with `|` at the caret, or a [`Written`] one.

use serde_json::Value;

use super::super::super::completion::{apply_completion, completions};
use super::super::super::schema::{SchemaBundle, build_schema};
use super::super::Language;
use super::Written;

pub(in super::super) fn burrow() -> SchemaBundle {
    build_schema(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap()
}

/// `marked` without its first `|`, and the caret's UTF-16 offset.
fn unmarked(marked: &str) -> (String, usize) {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    let cursor = source[..at].encode_utf16().count();
    (source, cursor)
}

/// The labels offered at the caret of `written` (as after Ctrl+Space).
pub(in super::super) fn offered_at(written: &Written) -> Vec<String> {
    completions(
        &written.text,
        written.cursor(),
        true,
        Some(&burrow()),
        true,
        written.language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

/// The labels offered at the caret (as after Ctrl+Space).
pub(in super::super) fn offered(language: Language, marked: &str) -> Vec<String> {
    let (source, _) = unmarked(marked);
    let byte = marked.find('|').expect("a | caret marker");
    offered_at(&Written {
        language,
        text: source,
        byte,
    })
}

/// The document after accepting `label` at the caret of `written`, with
/// the selection left (anchor, caret) as UTF-16 offsets, or why it could
/// not be.
pub(in super::super) fn accepted_at(
    written: &Written,
    label: &str,
) -> Result<(String, usize, usize), String> {
    let options = completions(
        &written.text,
        written.cursor(),
        true,
        Some(&burrow()),
        true,
        written.language.behavior(),
    );
    let option = options
        .iter()
        .find(|option| option.label == label)
        .ok_or_else(|| {
            format!(
                "{label} not offered; offered {:?}",
                options.iter().map(|o| &o.label).collect::<Vec<_>>()
            )
        })?;
    let (text, anchor, caret) = apply_completion(&written.text, option);
    let (anchor, caret) = (
        crate::editor::text::utf16_of(&text, anchor),
        crate::editor::text::utf16_of(&text, caret),
    );
    Ok((text, anchor, caret))
}

/// The document after accepting `label` at the caret, with the selection
/// left (anchor, caret) as UTF-16 offsets, or why it could not be.
pub(in super::super) fn accepted_with_selection(
    language: Language,
    marked: &str,
    label: &str,
) -> Result<(String, usize, usize), String> {
    let (source, _) = unmarked(marked);
    let byte = marked.find('|').expect("a | caret marker");
    accepted_at(
        &Written {
            language,
            text: source,
            byte,
        },
        label,
    )
}

/// The document after accepting `label` at the caret, or why it could not be.
pub(in super::super) fn accepted(
    language: Language,
    marked: &str,
    label: &str,
) -> Result<String, String> {
    accepted_with_selection(language, marked, label).map(|(text, _, _)| text)
}

pub(in super::super) fn parsed(language: Language, text: &str) -> Result<Value, String> {
    language
        .parse(text)
        .map_err(|error| format!("does not parse: {error}"))
}

/// The keys of the object at `path` (keys only) in `value`.
pub(in super::super) fn keys_at(value: &Value, path: &[&str]) -> Vec<String> {
    let mut current = value;
    for segment in path {
        current = match current.get(segment) {
            Some(next) => next,
            None => return Vec::new(),
        };
    }
    current
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}
