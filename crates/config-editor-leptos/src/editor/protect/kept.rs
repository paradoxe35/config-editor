//! What every edit the pipeline makes keeps, checked after each one in the
//! test builds: the guard is the one gate every user edit passes
//! (`edit::pipeline`), and the spans it judged are where the followers put
//! them with the same text. An edit path that reached the text without the
//! gate, or that the gate judged otherwise than the followers carry it,
//! fails every test driving it.
//!
//! Kept by an edit the gate took: each host range's text (as followed),
//! each placeholder no edit may cut (taken whole or kept whole), and, in a
//! text that does not parse, every locked field (carried, held or
//! unplaced: never dropped).

use leptos::prelude::*;

use super::super::state::EditorState;
use super::super::text::{TextRange, TextSize, slice};
use super::super::text_change::edited_bytes;
use super::tokens::cuts;

/// The text of each of `ranges` in `text`.
fn texts(text: &str, ranges: &[TextRange]) -> Vec<String> {
    ranges
        .iter()
        .map(|range| slice::slice(text, *range).unwrap_or_default().to_string())
        .collect()
}

/// The spans of the text before an edit.
pub(crate) struct Spans {
    epoch: u64,
    /// The host's ranges on screen, each with its text.
    ranges: Vec<(TextRange, String)>,
    /// The placeholders no edit may cut.
    tokens: Vec<TextRange>,
    /// The locked fields, each with its value's text.
    fields: Vec<(String, Vec<String>)>,
}

impl EditorState {
    /// The spans of `text` (the text on screen, before an edit).
    pub(crate) fn spans_before(&self, text: &str) -> Spans {
        let ranges = self.protect().ranges.with_untracked(|ranges| {
            ranges
                .iter()
                .map(|range| {
                    (
                        *range,
                        slice::slice(text, *range).unwrap_or_default().to_string(),
                    )
                })
                .collect()
        });
        let (epoch, fields) = self.protect().protected.with_untracked(|protected| {
            (
                protected.epoch,
                protected
                    .locks
                    .iter()
                    .filter_map(|lock| Some((lock.pointer.clone()?, texts(text, &lock.ranges))))
                    .collect(),
            )
        });
        Spans {
            epoch,
            ranges,
            tokens: self.solid_tokens(text),
            fields,
        }
    }

    /// Panics unless the edit from `before` to `after`, its caret at
    /// `caret`, which the gate took, kept `spans` (see the module). An undo
    /// or redo (`stepped`) goes back to a text as it was, placeholders
    /// typed whole or not.
    pub(crate) fn assert_spans_kept(
        &self,
        spans: &Spans,
        before: &str,
        after: &str,
        caret: TextSize,
        stepped: bool,
    ) {
        let followed = self.protect().ranges.get_untracked();
        if followed.len() == spans.ranges.len() {
            // An empty range locks nothing (one whose text an edit the host
            // has not heard changed): an edit writing its text back places
            // it on it again.
            for ((old, text), new) in spans.ranges.iter().zip(&followed) {
                if old.is_empty() {
                    continue;
                }
                assert!(
                    slice::slice(after, *new) == Some(text.as_str()),
                    "a host range {old:?} holds other text after an edit the guard took: \
                     {before:?} -> {after:?}"
                );
            }
        }
        let region = edited_bytes(before, after, caret);
        let rewritten =
            super::rewritten::rewritten_region(&spans.tokens, before, after, &region).is_some();
        if !rewritten
            && !stepped
            && let Some(cut) = spans.tokens.iter().find(|token| cuts(token, &region))
        {
            panic!("a placeholder {cut:?} cut by an edit the guard took: {before:?} -> {after:?}");
        }
        let lost = self.protect().protected.with_untracked(|protected| {
            (protected.epoch == spans.epoch)
                .then(|| {
                    spans.fields.iter().find(|(field, text)| {
                        // Locked under its pointer or the other one of an
                        // element written once in one text and repeated in
                        // the other (XML), or its text locked under another.
                        let named = |pointer: &str| {
                            super::guard::named(self.behavior(), (before, after), pointer, field)
                        };
                        !protected.locks.iter().any(|lock| {
                            lock.pointer.as_deref().is_some_and(named)
                                || texts(after, &lock.ranges) == *text
                        }) && !protected.held.iter().any(|pointer| named(pointer))
                            && !protected.unplaced.iter().any(|(pointer, _)| named(pointer))
                    })
                })
                .flatten()
                .map(|(field, _)| field.clone())
        });
        // A text that parses places its fields afresh (a list item a range
        // covers at another index, an XML element made a list): one that
        // does not carries each one from the text before. (Read without the
        // parse cache, which counts the work an edit costs.)
        if let Some(field) = lost
            && self.behavior().parse(after).is_err()
        {
            panic!("the lock of {field} lost by an edit the guard took: {before:?} -> {after:?}");
        }
    }
}
