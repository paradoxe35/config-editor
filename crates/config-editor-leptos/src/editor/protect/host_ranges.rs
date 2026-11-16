//! The host's read-only ranges in its own text while editing as another
//! language. Each conversion back writes a new host text, so the ranges are
//! followed from the text they were placed in to each new one (the lines
//! the conversion kept move with them, as an edit on screen moves them):
//! read at their first positions in a text with a key added above, they
//! would cover the field above. A range whose lines the conversion wrote
//! anew (its keys reordered, a separator added after it) is placed again
//! where the fields it covered are written in the new text: a conversion
//! writes fields, so their paths are what it keeps.

use std::ops::Range;

use leptos::prelude::*;
use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::super::paths::{field_range, with_spans};
use super::carry::Moves;
use super::follow::follow_ranges;
use super::locate::{LineIndex, lookup};
use super::save::range_pointers;
use crate::editor::paths::TextRange;
use crate::editor::work_count::{Work, note};

/// A host text and the host's ranges as UTF-16 offsets of it.
#[derive(Clone, Copy)]
pub(super) struct HostRanges(StoredValue<(String, Vec<Range<usize>>)>);

impl HostRanges {
    /// `ranges` placed in `host`.
    pub(super) fn new(host: &str, ranges: &[TextRange]) -> Self {
        HostRanges(StoredValue::new((host.to_string(), offsets(host, ranges))))
    }

    /// Places the host's `ranges` in its text `host` afresh (it gave either).
    pub(super) fn place(self, host: &str, ranges: &[TextRange]) {
        self.0.set_value((host.to_string(), offsets(host, ranges)));
    }

    /// Takes `ranges`, already followed into `text` (the host's language on
    /// screen, left for another).
    pub(super) fn keep(self, text: &str, ranges: Vec<Range<usize>>) {
        self.0.set_value((text.to_string(), ranges));
    }

    /// The ranges in `host` (written in `behavior`, read with `reading`),
    /// followed there from the text they were last placed in or followed to.
    pub(super) fn in_text(
        self,
        behavior: &dyn ConfigLanguage,
        host: &str,
        reading: Option<&Value>,
    ) -> Vec<Range<usize>> {
        let followed = self.0.with_value(|(text, ranges)| {
            if text == host || ranges.is_empty() {
                return None;
            }
            note(Work::RangesFollowed);
            Some(follow(behavior, text, host, ranges, reading))
        });
        match followed {
            Some(ranges) => {
                self.0.set_value((host.to_string(), ranges.clone()));
                ranges
            }
            None => self.0.with_value(|(_, ranges)| ranges.clone()),
        }
    }
}

/// `ranges` of `text` in `host`, a text written from it: by position where
/// their text was kept, else at the fields they covered when those read
/// the same in `host`, else what is left of them.
fn follow(
    behavior: &dyn ConfigLanguage,
    text: &str,
    host: &str,
    ranges: &[Range<usize>],
    reading: Option<&Value>,
) -> Vec<Range<usize>> {
    let moves = Moves::new(text, host);
    let mut placed = Vec::with_capacity(ranges.len());
    let mut rewritten = Vec::new();
    for range in ranges {
        match moves.range(range) {
            Some(range) => placed.push(range),
            None => rewritten.push(range.clone()),
        }
    }
    if rewritten.is_empty() {
        return placed;
    }
    match at_fields(behavior, text, host, &rewritten, reading) {
        Some(fields) => placed.extend(fields),
        None => placed.extend(rewritten.iter().map(|range| {
            let shifted = follow_ranges(std::slice::from_ref(range), text, host, 0, None);
            shifted.into_iter().next().unwrap_or(range.clone())
        })),
    }
    placed.sort_by_key(|range| (range.start, range.end));
    placed.dedup();
    placed
}

/// Where the fields `ranges` cover in `text` are written in `host`: `None`
/// unless both texts parse, the ranges cover a field, and each reads in
/// `host` as in `text` (a field at another index of a reordered list is
/// not the one covered).
fn at_fields(
    behavior: &dyn ConfigLanguage,
    text: &str,
    host: &str,
    ranges: &[Range<usize>],
    reading: Option<&Value>,
) -> Option<Vec<Range<usize>>> {
    let old = codec::read(behavior, text, reading).ok()?;
    let new = codec::read(behavior, host, reading).ok()?;
    let pointers = range_pointers(behavior, text, &old, ranges);
    let same = |pointer: &String| {
        lookup(&old, pointer).is_some_and(|value| lookup(&new, pointer) == Some(value))
    };
    if pointers.is_empty() || !pointers.iter().all(same) {
        return None;
    }
    let lines = LineIndex::new(host);
    with_spans(behavior, host, &pointers, || {
        pointers
            .iter()
            .map(|pointer| field_range(behavior, host, pointer).map(|range| lines.range(range)))
            .collect()
    })
}

/// `ranges` (positions in `text`) as UTF-16 offsets.
pub(super) fn offsets(text: &str, ranges: &[TextRange]) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(text);
    ranges.iter().map(|range| lines.range(*range)).collect()
}
