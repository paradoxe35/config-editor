//! The host's read-only ranges in its own text while editing as another
//! language. Each conversion back writes a new host text, so the ranges are
//! followed from the text they were placed in to each new one (the lines
//! the conversion kept move with them, as an edit on screen moves them):
//! read at their first positions in a text with a key added above, they
//! would cover the field above.

use std::ops::Range;

use leptos::prelude::*;

use super::carry::Moves;
use super::follow::follow_ranges;
use super::locate::LineIndex;
use crate::editor::paths::TextRange;
use crate::editor::state::EditorState;
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

    /// The ranges in `host`, followed there from the text they were last
    /// placed in or followed to.
    pub(super) fn in_text(self, host: &str) -> Vec<Range<usize>> {
        let followed = self.0.with_value(|(text, ranges)| {
            if text == host || ranges.is_empty() {
                return None;
            }
            note(Work::RangesFollowed);
            let moves = Moves::new(text, host);
            Some(
                ranges
                    .iter()
                    .map(|range| {
                        moves.range(range).unwrap_or_else(|| {
                            let shifted = follow_ranges(std::slice::from_ref(range), text, host, 0);
                            shifted.into_iter().next().unwrap_or(range.clone())
                        })
                    })
                    .collect::<Vec<_>>(),
            )
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

/// `ranges` (positions in `text`) as UTF-16 offsets.
pub(super) fn offsets(text: &str, ranges: &[TextRange]) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(text);
    ranges.iter().map(|range| lines.range(*range)).collect()
}

/// UTF-16 `ranges` of `text` as positions in it.
fn positions(text: &str, ranges: &[Range<usize>]) -> Vec<TextRange> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(text);
    ranges
        .iter()
        .map(|range| TextRange::new(lines.position(range.start), lines.position(range.end)))
        .collect()
}

impl EditorState {
    /// The host's ranges where they are now, followed through every edit
    /// since they were placed, as positions in the host's text (the text
    /// on screen, or the host's own while editing as another language):
    /// what a host replacing the text without new ranges moves them from
    /// (QA 37, FW37-2: placed again where they were given, in the text
    /// that replaced the one they followed, a lock covered another line).
    pub(crate) fn host_ranges_now(&self) -> Vec<TextRange> {
        let host = self.config().host_language == self.sync().edit_language.get_untracked();
        if host {
            let ranges = self.protect().ranges.get_untracked();
            self.doc()
                .text
                .with_untracked(|text| positions(text, &ranges))
        } else {
            self.sync().host_text.with_value(|text| {
                let ranges = self.protect().in_host.in_text(text);
                positions(text, &ranges)
            })
        }
    }
}
