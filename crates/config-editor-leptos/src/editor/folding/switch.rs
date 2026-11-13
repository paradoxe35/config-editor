//! Closed folds through an Edit-as switch: the document is another
//! language's lines, so each fold is carried by the field its first line
//! names (`web:` folded is `"web": {` folded) and closed again where that
//! field folds in the new text.

use super::super::Language;
use super::super::fold_ranges::FoldRange;
use super::super::paths::{TextPosition, offset_of, paths_at, split_line, with_spans};
use super::super::state::EditorState;
use leptos::prelude::*;
use std::collections::HashSet;

/// The fields `folds` (closed in `text`, in `language`) start on.
pub(crate) fn folded_fields(folds: &[FoldRange], text: &str, language: Language) -> Vec<String> {
    let starts: Vec<usize> = folds.iter().map(|fold| fold.start).collect();
    line_fields(text, language, &starts)
        .into_iter()
        .flatten()
        .collect()
}

/// The fold ranges of `text` (in `language`) that start on one of
/// `fields`, the outermost of those starting on the same field.
pub(crate) fn folds_of_fields(fields: &[String], text: &str, language: Language) -> Vec<FoldRange> {
    if fields.is_empty() {
        return Vec::new();
    }
    let ranges = language.behavior().fold_ranges(text);
    let starts: Vec<usize> = ranges.iter().map(|range| range.start).collect();
    let wanted: HashSet<&str> = fields.iter().map(String::as_str).collect();
    let mut taken: HashSet<String> = HashSet::new();
    let mut folds: Vec<FoldRange> = Vec::new();
    for (range, field) in ranges.iter().zip(line_fields(text, language, &starts)) {
        let Some(field) = field else {
            continue;
        };
        if wanted.contains(field.as_str()) && taken.insert(field) {
            folds.push(*range);
        }
    }
    folds.sort_unstable();
    folds
}

/// The field each of `lines` (0-based) opens: of the paths at its first
/// character and at its end (a TOML `[db.pool]` names `/db` at its
/// start), the deepest holding a map or list in the document read; else
/// the first. The text is read once for all of them (one parse, one walk
/// of the language's paths, its lines split once).
fn line_fields(text: &str, language: Language, lines: &[usize]) -> Vec<Option<String>> {
    if lines.is_empty() {
        return Vec::new();
    }
    let value = language.parse(text).ok();
    let behavior = language.behavior();
    with_spans(behavior, text, &[], || {
        // Per line: the UTF-16 offsets of its first character and its end.
        let places: Vec<Option<(usize, usize)>> = lines
            .iter()
            .map(|&line| {
                let written = split_line(text, line)?;
                let indent = written
                    .chars()
                    .take_while(|character| character.is_whitespace())
                    .count();
                let end = written.trim_end().chars().count();
                let at = |column: usize| offset_of(text, TextPosition::new(line + 1, column), true);
                Some((at(indent + 2), at(end)))
            })
            .collect();
        let cursors: Vec<usize> = places
            .iter()
            .flatten()
            .flat_map(|(first, end)| [*first, *end])
            .collect();
        let mut paths = paths_at(behavior, text, &cursors).into_iter();
        let container = |pointer: &String| {
            value
                .as_ref()
                .and_then(|value| value.pointer(pointer))
                .is_some_and(|held| held.is_object() || held.is_array())
        };
        places
            .iter()
            .map(|place| {
                place.as_ref()?;
                let first = paths.next().flatten().map(|path| path.to_pointer());
                let last = paths.next().flatten().map(|path| path.to_pointer());
                let first = first?;
                Some(match last {
                    Some(last) if last.len() > first.len() && container(&last) => last,
                    _ => first,
                })
            })
            .collect()
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
