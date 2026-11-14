//! An edit leaving the text unparseable, near placeholders the host gave
//! whole: each one's value is held as a read-only field's is through a
//! syntax error, for its text alone (see `guard::check_values`). Its value
//! is found where the language reads it written, all of them in one read
//! of the text, and pinned to the string the host gave; only a field the
//! language finds no value written for is placed as a lock is (`protect`).
//! So the check costs a reading of the text and work for each placeholder
//! near the edit, never a pass over the text for each (Ctrl+A Tab in a
//! long broken text was seconds).

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use serde_json::Value;

use super::super::super::codec::scalar_range;
use super::super::super::paths::{FieldPath, path_at, with_spans, written_value_span};
use super::super::super::text_change::{changed_bytes, utf16_len};
use super::super::guard::check_values;
use super::super::locate::LineIndex;
use super::super::locks::{Lock, Protected, Sources, protect};
use super::super::texts::Reason;
use super::super::tokens::Known;
use super::Placeholders;

/// Non-blank lines past a placeholder's own read for an edit: a closing
/// line, and the lines a deletion or a move would bring into the value.
const LINES_AFTER: usize = 3;

/// A placeholder: its UTF-16 range in the text before, and the host's
/// token it is.
type Live<'k> = (Range<usize>, &'k Known);

impl Placeholders<'_> {
    /// The field of a placeholder near the edit that the edit, leaving the
    /// text unparseable, changes as it would a read-only field's value.
    pub(super) fn held_while_broken(&self, before: &str, after: &str) -> Option<String> {
        let (start, old_end, new_end) = changed_bytes(before, after);
        let lines = LineIndex::new(before);
        let near_ranges: Vec<&Range<usize>> = self
            .tokens
            .iter()
            .filter(|range| near(&lines, range, start, old_end))
            .collect();
        // Tokens given whole near the edit in none of their fields (a
        // parent key renamed, a line moved) are held as in a field of
        // their own where they are written (see `moved`).
        let moved = self.moved_near(before, start, old_end);
        if near_ranges.is_empty() && moved.is_empty() {
            return None;
        }
        // Each placeholder's field from where it is written (the text read
        // once for all of them); one written where no field given its
        // token is (a text that reads wrong around it) keeps its line.
        let ((mut near, mut placed), stand_ins) = with_spans(self.behavior, before, &[], || {
            (
                self.placed(before, &lines, &near_ranges),
                self.moved_alone(before, moved),
            )
        });
        for (range, known) in &stand_ins {
            near.push((range.clone(), known));
            placed.push(true);
        }
        if near.is_empty() {
            return None;
        }
        let mut seen = HashSet::new();
        let pointers: Vec<String> = near
            .iter()
            .zip(&placed)
            .filter(|(_, placed)| **placed)
            .map(|((_, known), _)| known.field.clone().unwrap_or_default())
            .filter(|pointer| seen.insert(pointer.clone()))
            .collect();
        let near: Vec<&Live<'_>> = near.iter().collect();
        // The fields' spans are read once for the placing and the check.
        with_spans(self.behavior, before, &pointers, || {
            let mut protected = self.value_locks(before, &lines, &near, &pointers);
            // A placeholder the edit takes away whole leaves its field free
            // (a secret replaced); renaming or moving the keys above one is
            // free.
            let removed = removed_by(before, after, (start, old_end, new_end));
            let holders = holders(&protected.locks, &near);
            // A placeholder whose field the language cannot find in the
            // text (it gives no spans, or it is not where its token is)
            // keeps its line as it is.
            let unfound = near.iter().zip(&holders).find(|((range, _), holder)| {
                holder.is_none() && !removed(range) && on_line(&lines, range, start, old_end)
            });
            if let Some(((_, known), _)) = unfound {
                return known.field.clone();
            }
            let mut kept = vec![false; protected.locks.len()];
            let mut freed = vec![false; protected.locks.len()];
            for ((range, _), holder) in near.iter().zip(&holders) {
                if let Some(lock) = holder {
                    kept[*lock] = true;
                    freed[*lock] |= removed(range);
                }
            }
            let mut index = 0;
            protected.locks.retain(|_| {
                index += 1;
                kept[index - 1] && !freed[index - 1]
            });
            let pointer = protected.locks.first()?.pointer.clone();
            check_values(
                self.behavior,
                self.reading_schema,
                &protected,
                before,
                after,
            )
            .err()
            .map(|blocked| blocked.pointer.or(pointer).unwrap_or_default())
        })
    }

    /// Each of `ranges` (UTF-16, in `before`) whose token is given whole,
    /// with the host's token in the field written there, or, where no
    /// field given that token is written there (`false`), the first one
    /// given it (named when its line is refused).
    fn placed(
        &self,
        before: &str,
        lines: &LineIndex<'_>,
        ranges: &[&Range<usize>],
    ) -> (Vec<Live<'_>>, Vec<bool>) {
        let mut near = Vec::new();
        let mut placed = Vec::new();
        for range in ranges {
            let token = &before[lines.byte_of(range.start)..lines.byte_of(range.end)];
            let Some(first) = self.index.first_of(token) else {
                continue;
            };
            let at = path_at(self.behavior, before, range.start)
                .and_then(|path| self.index.in_field(token, &path.to_pointer()));
            near.push(((*range).clone(), &self.known[at.unwrap_or(first)]));
            placed.push(at.is_some());
        }
        (near, placed)
    }

    /// The fields at `pointers` locked for their values alone: each value
    /// where the language reads it written in `before` (the spans of
    /// `pointers` read at once), pinned to the string the host gave (`near`
    /// holds its placeholder); a field it finds no value written for is
    /// placed as a lock is (`protect`: its key may be renamed, as when the
    /// text parses).
    fn value_locks(
        &self,
        before: &str,
        lines: &LineIndex<'_>,
        near: &[&Live<'_>],
        pointers: &[String],
    ) -> Protected {
        let mut found = Vec::new();
        let mut unfound = Vec::new();
        for pointer in pointers {
            let value = written_value_span(self.behavior, before, pointer)
                .and_then(|span| scalar_range(self.behavior, before, span));
            match value {
                Some(value) => found.push((pointer, value)),
                None => unfound.push(FieldPath::from(pointer.as_str())),
            }
        }
        let mut protected = if unfound.is_empty() {
            Protected::default()
        } else {
            protect(
                self.behavior,
                before,
                Sources {
                    reading_schema: self.reading_schema,
                    paths: &unfound,
                    ..Sources::default()
                },
            )
        };
        for lock in &mut protected.locks {
            lock.guards.clear();
        }
        let wholes: HashMap<&str, &str> = near
            .iter()
            .filter_map(|(_, known)| Some((known.field.as_deref()?, known.whole.as_deref()?)))
            .collect();
        for (pointer, value) in found {
            protected.locks.push(Lock {
                pointer: Some(pointer.clone()),
                key: None,
                ranges: std::iter::once(lines.units_of(value.start)..lines.units_of(value.end))
                    .collect(),
                guards: Vec::new(),
                reason: Reason::Field,
            });
            if let Some(whole) = wholes.get(pointer.as_str()) {
                protected
                    .pins
                    .push((pointer.clone(), Value::String((*whole).to_string())));
            }
        }
        protected
    }
}

/// For each of `near`, the index in `locks` of the lock holding it in one
/// of its stretches (those of different fields never overlap: the one
/// starting last before it is the one).
fn holders(locks: &[Lock], near: &[&Live<'_>]) -> Vec<Option<usize>> {
    let mut stretches: Vec<(&Range<usize>, usize)> = locks
        .iter()
        .enumerate()
        .flat_map(|(index, lock)| lock.ranges.iter().map(move |range| (range, index)))
        .collect();
    stretches.sort_by_key(|(range, _)| range.start);
    near.iter()
        .map(|(token, _)| {
            let at = stretches.partition_point(|(range, _)| range.start <= token.start);
            let (range, lock) = stretches.get(at.checked_sub(1)?)?;
            (token.end <= range.end).then_some(*lock)
        })
        .collect()
}

/// Whether the edit from `before` to `after` (`changed`: the bytes it
/// replaced, and where the new ones end) takes away the placeholder at a
/// UTF-16 range of `before`: it changes text there (edits never cut one, so
/// it went whole), and its text is not written again where the edit wrote
/// (a line moved or copied keeps it). The two texts may share the token's
/// last characters with what replaced it: its length around is read too.
fn removed_by<'a>(
    before: &'a str,
    after: &'a str,
    (start, old_end, new_end): (usize, usize, usize),
) -> impl Fn(&Range<usize>) -> bool + 'a {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let (first, last) = (utf16_len(&before[..start]), utf16_len(&before[..old_end]));
    let new_last = utf16_len(&after[..new_end]);
    move |range: &Range<usize>| {
        let touched = if first == last {
            range.start < first && first < range.end
        } else {
            first < range.end && range.start < last
        };
        if !touched {
            return false;
        }
        let token = &old[range.clone()];
        let window =
            &new[first.saturating_sub(token.len())..(new_last + token.len()).min(new.len())];
        !window
            .windows(token.len().max(1))
            .any(|found| found == token)
    }
}

/// Whether the edit replacing bytes `start..end` of the text `lines`
/// splits changes the line of the placeholder at UTF-16 `token`.
fn on_line(lines: &LineIndex<'_>, token: &Range<usize>, start: usize, end: usize) -> bool {
    let first = lines.line_bounds(lines.line_of(token.start));
    let last = lines.line_bounds(lines.line_of(token.end));
    start <= lines.byte_of(last.end) && lines.byte_of(first.start) <= end
}

/// Whether the edit replacing bytes `start..end` of the text `lines`
/// splits is on the lines around the placeholder at UTF-16 `token`: from
/// the last line holding code above it (a header) to the
/// [`LINES_AFTER`]th below it.
fn near(lines: &LineIndex<'_>, token: &Range<usize>, start: usize, end: usize) -> bool {
    let line = lines.line_of(token.start);
    // Up through blank lines to a line holding code.
    let mut from = line.saturating_sub(1);
    while from > 0 && lines.line_text(from).trim().is_empty() {
        from -= 1;
    }
    let mut to = lines.line_of(token.end);
    let mut counted = 0;
    while counted < LINES_AFTER && to + 1 < lines.line_count() {
        to += 1;
        if !lines.line_text(to).trim().is_empty() {
            counted += 1;
        }
    }
    let from = lines.byte_of(lines.line_bounds(from).start);
    let to = lines.byte_of(lines.line_bounds(to).end);
    start <= to && from <= end
}
