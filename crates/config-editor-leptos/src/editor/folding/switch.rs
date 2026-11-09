//! Closed folds through an Edit-as switch: the document is another
//! language's lines, so each fold is carried by the field its first line
//! names (`web:` folded is `"web": {` folded) and closed again where that
//! field folds in the new text.

use super::super::Language;
use super::super::fold_ranges::FoldRange;
use super::super::paths::TextPosition;
use super::super::state::EditorState;
use leptos::prelude::*;
use serde_json::Value;

/// The fields `folds` (closed in `text`, in `language`) start on.
pub(crate) fn folded_fields(folds: &[FoldRange], text: &str, language: Language) -> Vec<String> {
    let value = language.parse(text).ok();
    folds
        .iter()
        .filter_map(|fold| line_field(text, language, fold.start, value.as_ref()))
        .collect()
}

/// The fold ranges of `text` (in `language`) that start on one of
/// `fields`, the outermost of those starting on the same field.
pub(crate) fn folds_of_fields(fields: &[String], text: &str, language: Language) -> Vec<FoldRange> {
    if fields.is_empty() {
        return Vec::new();
    }
    let value = language.parse(text).ok();
    let mut folds: Vec<FoldRange> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for range in language.behavior().fold_ranges(text) {
        let Some(field) = line_field(text, language, range.start, value.as_ref()) else {
            continue;
        };
        if fields.contains(&field) && !taken.contains(&field) {
            taken.push(field);
            folds.push(range);
        }
    }
    folds.sort_unstable();
    folds
}

/// The field the line `line` (0-based) opens: of the paths at its first
/// character and at its end (a TOML `[db.pool]` names `/db` at its
/// start), the deepest holding a map or list in `value`, the document
/// read; else the first.
fn line_field(
    text: &str,
    language: Language,
    line: usize,
    value: Option<&Value>,
) -> Option<String> {
    let written = text.split('\n').nth(line)?;
    let indent = written
        .chars()
        .take_while(|character| character.is_whitespace())
        .count();
    let end = written.trim_end().chars().count();
    let at = |column: usize| {
        language
            .path_at(text, TextPosition::new(line + 1, column))
            .map(|path| path.to_pointer())
    };
    let first = at(indent + 2)?;
    let container = |pointer: &String| {
        value
            .and_then(|value| value.pointer(pointer))
            .is_some_and(|held| held.is_object() || held.is_array())
    };
    Some(match at(end) {
        Some(last) if last.len() > first.len() && container(&last) => last,
        _ => first,
    })
}

impl EditorState {
    /// The fields the closed folds start on, in `language`'s `text`.
    pub(crate) fn folded_fields(&self, text: &str, language: Language) -> Vec<String> {
        self.fold()
            .folded
            .with_untracked(|folds| folded_fields(folds, text, language))
    }

    /// Closes the folds of the document now shown (in `language`) that
    /// start on `fields`, and shows it. Returns whether the textarea's text
    /// had to be rewritten (its selection is then lost).
    pub(crate) fn fold_fields(&self, fields: &[String], language: Language) -> bool {
        if fields.is_empty() {
            return false;
        }
        let folds = self
            .doc()
            .text
            .with_untracked(|text| folds_of_fields(fields, text, language));
        self.settle_folds(Some(folds))
    }
}
