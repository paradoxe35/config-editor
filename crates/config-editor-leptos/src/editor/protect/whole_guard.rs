//! The check every edit passes for the placeholders the host gave as a
//! field's whole value (see `glued`), judged by what the edit makes, never
//! by its shape, so every way text changes is held alike (typing, Enter,
//! indenting, Tab, line commands, cut, paste, drag, Replace All, undo):
//! - an edit changing one scalar on a line no such placeholder is on, whose
//!   path is none of their fields, nor inside or holding one, in a text that
//!   parses, leaves each value as it was: kept without reading the document
//!   (`reach`), so a placeholder costs a keystroke away from it nothing;
//! - else, when the edited text parses, each field holding its placeholder
//!   holds the value it held: a line moved into a block scalar or heredoc,
//!   or a closing line deleted, gives it more, a `|` block's last line
//!   break joined away (in an XML view) takes some, and either is refused.
//!   A value that already had text beside it may only lose some; after a
//!   repair (the text before did not parse) the token must stand alone;
//! - else (the edited text does not parse) the field is held as a read-only
//!   field is through a syntax error, by the lock check (`guard`): its
//!   value's text is kept, nothing glued on its ends, no line joining it or
//!   indented under it; its key stays free to rename or move (the value
//!   moves with it). An edit taking the placeholder away whole (a
//!   secret replaced) is free. Read near the placeholders only: an edit
//!   elsewhere cannot write beside one, and the repair is checked when the
//!   text parses.

use std::ops::Range;

use serde_json::Value;

use super::super::codec::scalar_range;
use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::paths::{FieldPath, written_value_span};
use super::super::text_change::{changed_bytes, utf16_len};
use super::glued::{glued_in, only_taken_from, value_of};
use super::guard::check_values;
use super::locate::LineIndex;
use super::locks::{Lock, Sources, protect};
use super::reach::reaches_none;
use super::texts::Reason;
use super::tokens::Known;

/// Non-blank lines past a placeholder's own read for an edit: a closing
/// line, and the lines a deletion or a move would bring into the value.
const LINES_AFTER: usize = 3;

/// The host's placeholders in the text before an edit.
pub(crate) struct Placeholders<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) reading_schema: Option<&'a Value>,
    pub(crate) known: &'a [Known],
    /// UTF-16 ranges of the real placeholders in the text before.
    pub(crate) tokens: &'a [Range<usize>],
}

/// What the check found: the field of a placeholder the edit writes text
/// beside, and whether the edited text parses (`None`: not read).
pub(crate) struct Verdict {
    pub(crate) glued: Option<String>,
    pub(crate) parses: Option<bool>,
}

impl Placeholders<'_> {
    /// The edit from `before` (which parses, when `before_parses`) to
    /// `after`.
    pub(crate) fn check(&self, before: &str, after: &str, before_parses: bool) -> Verdict {
        let unread = Verdict {
            glued: None,
            parses: None,
        };
        let live = self.live(before);
        if live.is_empty() || before == after {
            return unread;
        }
        if before_parses
            && reaches_none(self.behavior, &self.reach_locks(&live), before, after).is_some()
        {
            return Verdict {
                glued: None,
                parses: Some(true),
            };
        }
        let parsed = parse_cache::parsed(self.behavior, after);
        match parsed.as_ref() {
            Ok(document) => Verdict {
                glued: self.glued_in_parsed(&live, before, document),
                parses: Some(true),
            },
            Err(_) => Verdict {
                glued: self.held_while_broken(&live, before, after),
                parses: Some(false),
            },
        }
    }

    /// Each real placeholder given whole, with its UTF-16 range in `before`
    /// and the host's token it is.
    fn live(&self, before: &str) -> Vec<(Range<usize>, &Known)> {
        if self.tokens.is_empty() || !self.known.iter().any(|known| known.whole.is_some()) {
            return Vec::new();
        }
        let units: Vec<u16> = before.encode_utf16().collect();
        self.tokens
            .iter()
            .flat_map(|range| {
                let written = String::from_utf16_lossy(
                    &units[range.start.min(units.len())..range.end.min(units.len())],
                );
                self.known
                    .iter()
                    .filter(move |known| {
                        known.whole.is_some() && known.field.is_some() && known.token == written
                    })
                    .map(move |known| (range.clone(), known))
            })
            .collect()
    }

    /// Each placeholder as a lock for the reach check: its token's text in
    /// its field.
    fn reach_locks(&self, live: &[(Range<usize>, &Known)]) -> Vec<Lock> {
        live.iter()
            .map(|(range, known)| Lock {
                pointer: known.field.clone(),
                key: None,
                ranges: vec![range.clone()],
                guards: Vec::new(),
                reason: Reason::Field,
            })
            .collect()
    }

    /// The field of a placeholder whose value the edited text (`document`)
    /// changes while still holding it: text beside it, or the whitespace a
    /// multi-line form gives it (a `|` block's last line break, joined away
    /// in an XML view). A value that already had text beside it may only
    /// lose some; after a repair (`before` does not parse) it holds the
    /// token alone, as the host wrote it or bare.
    fn glued_in_parsed(
        &self,
        live: &[(Range<usize>, &Known)],
        before: &str,
        document: &Value,
    ) -> Option<String> {
        // Read the text before only for a value not as the host wrote it.
        let mut changed = live
            .iter()
            .map(|(_, known)| *known)
            .filter(|known| {
                let now = value_of(document, known);
                now.contains(&known.token) && known.whole.as_deref() != Some(now)
            })
            .peekable();
        changed.peek()?;
        let earlier = parse_cache::parsed(self.behavior, before);
        let earlier = earlier.as_ref().as_ref().ok();
        changed
            .find(|known| {
                let now = value_of(document, known);
                match earlier {
                    Some(earlier) if glued_in(earlier, known) => {
                        !only_taken_from(value_of(earlier, known), now)
                    }
                    Some(earlier) if value_of(earlier, known).contains(&known.token) => {
                        value_of(earlier, known) != now
                    }
                    _ => glued_in(document, known),
                }
            })
            .and_then(|known| known.field.clone())
    }

    /// The field of a placeholder near the edit that the edit, leaving the
    /// text unparseable, changes as it would a read-only field's value.
    fn held_while_broken(
        &self,
        live: &[(Range<usize>, &Known)],
        before: &str,
        after: &str,
    ) -> Option<String> {
        let (start, old_end, new_end) = changed_bytes(before, after);
        let near: Vec<&(Range<usize>, &Known)> = live
            .iter()
            .filter(|(range, _)| near(before, range, start, old_end))
            .collect();
        if near.is_empty() {
            return None;
        }
        let mut fields: Vec<FieldPath> = Vec::new();
        for (_, known) in &near {
            let path = FieldPath::from(known.field.as_deref().unwrap_or_default());
            if !fields.contains(&path) {
                fields.push(path);
            }
        }
        let mut protected = protect(
            self.behavior,
            before,
            Sources {
                reading_schema: self.reading_schema,
                paths: &fields,
                ..Sources::default()
            },
        );
        // A placeholder the edit takes away whole leaves its field free (a
        // secret replaced); renaming or moving the keys above one is free.
        let removed = removed_by(before, after, (start, old_end, new_end));
        // A placeholder whose field the language cannot find in the text
        // (it gives no spans) keeps its line as it is.
        let unfound = near.iter().find(|(range, _)| {
            !protected.locks.iter().any(|lock| {
                lock.ranges
                    .iter()
                    .any(|stretch| stretch.start <= range.start && range.end <= stretch.end)
            }) && !removed(range)
                && on_line(before, range, start, old_end)
        });
        if let Some((_, known)) = unfound {
            return known.field.clone();
        }
        protected.locks.retain(|lock| {
            let held: Vec<&Range<usize>> = near
                .iter()
                .map(|(range, _)| range)
                .filter(|range| {
                    lock.ranges
                        .iter()
                        .any(|stretch| stretch.start <= range.start && range.end <= stretch.end)
                })
                .collect();
            !held.is_empty() && !held.iter().any(|range| removed(range))
        });
        if protected.locks.is_empty() {
            return None;
        }
        // The value alone is held: its key (an XML element's tags) may be
        // renamed, as when the text parses.
        let lines = LineIndex::new(before);
        for lock in &mut protected.locks {
            let value = lock.pointer.as_deref().and_then(|pointer| {
                written_value_span(self.behavior, before, pointer)
                    .and_then(|span| scalar_range(self.behavior, before, span))
            });
            if let Some(value) = value {
                let units = lines.units_of(value.start)..lines.units_of(value.end);
                lock.ranges = std::iter::once(units).collect();
                lock.key = None;
            }
            lock.guards.clear();
        }
        let pointer = protected.locks[0].pointer.clone();
        check_values(
            self.behavior,
            self.reading_schema,
            &protected,
            before,
            after,
        )
        .err()
        .map(|blocked| blocked.pointer.or(pointer).unwrap_or_default())
    }
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

/// Whether the edit replacing bytes `start..end` of `before` changes the
/// line of the placeholder at UTF-16 `token`.
fn on_line(before: &str, token: &Range<usize>, start: usize, end: usize) -> bool {
    use super::super::keyboard::utf16_to_byte_index;
    let (from, to) = (
        utf16_to_byte_index(before, token.start),
        utf16_to_byte_index(before, token.end),
    );
    let line_start = before[..from].rfind('\n').map_or(0, |at| at + 1);
    let line_end = before[to..].find('\n').map_or(before.len(), |at| to + at);
    start <= line_end && line_start <= end
}

/// Whether the edit replacing bytes `start..end` of `before` is on the
/// lines around the placeholder at UTF-16 `token`: from the last line
/// holding code above it (a header) to the [`LINES_AFTER`]th below it.
fn near(before: &str, token: &Range<usize>, start: usize, end: usize) -> bool {
    let byte = super::super::keyboard::utf16_to_byte_index(before, token.start);
    let line_start = before[..byte].rfind('\n').map_or(0, |at| at + 1);
    let from = before[..line_start.saturating_sub(1)]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    // Up through blank lines to a line holding code.
    let mut from = from;
    while from > 0
        && before[from..]
            .split('\n')
            .next()
            .is_some_and(|line| line.trim().is_empty())
    {
        from = before[..from - 1].rfind('\n').map_or(0, |at| at + 1);
    }
    let mut to = before[byte..]
        .find('\n')
        .map_or(before.len(), |at| byte + at);
    let mut counted = 0;
    while counted < LINES_AFTER && to < before.len() {
        let next = before[to + 1..]
            .find('\n')
            .map_or(before.len(), |at| to + 1 + at);
        if !before[to + 1..next].trim().is_empty() {
            counted += 1;
        }
        to = next;
    }
    start <= to && from <= end
}
