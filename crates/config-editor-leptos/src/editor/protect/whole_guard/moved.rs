//! Placeholders written where no field the host gave them is: one fixed
//! mask whose field's parent key was renamed (which of the alike fields it
//! was cannot be told), or whose line was moved into another block. Such a
//! token is plain text there (and flagged), yet it is still the host's
//! token standing for a whole secret: nothing may be written beside it, or
//! the host would hear `«…»MY` as a new value. They are found by their text
//! near the edit (a few lines each way, never a pass of the document), and
//! each is judged by its holder (see `holder`), the string written around
//! it, before and after the edit: a holder that held it alone may not get
//! text beside it, nor become a list or map holding it; one that already
//! had text beside it is the user's (a mask pasted into a note). Alone is
//! the token as is, or in its host field's form.

use std::cell::OnceCell;
use std::ops::Range;

use serde_json::Value;

use super::super::super::language::Repetition;
use super::super::super::parse_cache;
use super::super::super::text::text_size;
use super::super::super::text_change::utf16_len;
use super::super::glued::{Relation, relation};
use super::super::holder::Holders;
use super::super::tokens::field_value;
use super::{Placeholders, Read};

/// Non-blank lines each way from an edit read for tokens: a multi-line
/// form's header or closing line, the lines a value runs over.
const LINES_AROUND: usize = 3;

/// A token written in a text: its byte range, and its UTF-16 start.
pub(super) type Written = (Range<usize>, usize);

/// The bytes of `text` from [`LINES_AROUND`] non-blank lines above byte
/// `start` to as many below byte `end`.
fn around(text: &str, start: usize, end: usize) -> Range<usize> {
    let mut from = text
        .get(..start)
        .unwrap_or_default()
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let mut counted = 0;
    while counted < LINES_AROUND && from > 0 {
        let line_start = text
            .get(..from - 1)
            .unwrap_or_default()
            .rfind('\n')
            .map_or(0, |at| at + 1);
        if !text
            .get(line_start..from)
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            counted += 1;
        }
        from = line_start;
    }
    let mut to = text
        .get(end..)
        .unwrap_or_default()
        .find('\n')
        .map_or(text.len(), |at| end + at);
    let mut counted = 0;
    while counted < LINES_AROUND && to < text.len() {
        let line_end = text
            .get(to + 1..)
            .unwrap_or_default()
            .find('\n')
            .map_or(text.len(), |at| to + 1 + at);
        if !text
            .get(to + 1..line_end)
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            counted += 1;
        }
        to = line_end;
    }
    from..to
}

impl Placeholders<'_> {
    /// Whether `text` is `token` alone: as is, or in the form the host's
    /// field gives it (whitespace a multi-line form writes around it).
    fn alone(&self, text: &str, token: &str) -> bool {
        text == token
            || self
                .index
                .first_of(token)
                .and_then(|at| self.known[at].whole.as_deref())
                == Some(text)
    }

    /// Whether the string at `pointer` is a field `token` was given in, a
    /// copy of one or inside one (judged there, see `glued_in_parsed`).
    fn in_its_fields(&self, token: &str, pointer: &str) -> bool {
        let repetition = self.behavior.repetition();
        self.index.of_token(token).any(|at| {
            self.known[at]
                .field
                .as_deref()
                .is_some_and(|field| relation(pointer, field, repetition).is_some())
        })
    }

    /// The tokens given whole written in `text` around bytes `start..end`,
    /// in order.
    pub(super) fn written_near(&self, text: &str, start: usize, end: usize) -> Vec<Written> {
        let window = around(text, start, end);
        let found = self
            .matcher
            .find_in(text.get(window.clone()).unwrap_or_default());
        if found.is_empty() {
            return Vec::new();
        }
        let mut units = utf16_len(text.get(..window.start).unwrap_or_default());
        let mut byte = window.start;
        let mut written = Vec::new();
        for range in found {
            let range = window.start + range.start..window.start + range.end;
            if self
                .index
                .first_of(text.get(range.clone()).unwrap_or_default())
                .is_none()
            {
                continue;
            }
            units += utf16_len(text.get(byte..range.start).unwrap_or_default());
            byte = range.start;
            written.push((range, units));
        }
        written
    }

    /// [`Self::written_near`] in the text before the edit, but the real
    /// placeholders (those in the fields the host gave them).
    pub(super) fn moved_near(&self, before: &str, start: usize, end: usize) -> Vec<Written> {
        let mut written = self.written_near(before, start, end);
        written.retain(|(range, _)| {
            self.tokens
                .binary_search_by_key(&text_size(range.start), |token| token.start())
                .is_err()
        });
        written
    }

    /// The pointer, where it is written now, of a token given whole near
    /// the edit from `before` to the text `read` reads (its holder query
    /// made only when a token is near) that is in none of its token's
    /// fields, and whose holder (see `holder`) the edit gave text beside it
    /// or made a list or map: a holder that already had text beside it is
    /// the user's (a mask pasted into a note), and so is one the edit wrote
    /// whole and new. `changed`: the bytes the edit replaced in `before`,
    /// and where the new ones end in `after`.
    pub(super) fn glued_where_moved(
        &self,
        before: &str,
        read: Read<'_, '_>,
        (start, old_end, new_end): (usize, usize, usize),
    ) -> Option<String> {
        let after = read.text;
        let near = self.written_near(after, start, new_end);
        if near.is_empty() {
            return None;
        }
        let behavior = self.behavior;
        let holders = read.holders(self.matcher, behavior);
        let parsed_before = parse_cache::parsed(behavior, before);
        let earlier = parsed_before.as_ref().as_ref().ok();
        let holders_before = OnceCell::new();
        let was_held = |range: &Range<usize>| {
            let earlier = earlier?;
            holders_before
                .get_or_init(|| Holders::read(behavior, before, earlier, self.matcher))
                .at(range)
                .cloned()
        };
        let written = after.get(start..new_end).unwrap_or_default();
        near.iter().find_map(|(range, _)| {
            let token = after.get(range.clone())?;
            let holder = holders.at(range)?;
            if self.in_its_fields(token, &holder.pointer) {
                return None;
            }
            let beside = !self.alone(holder.text, token);
            let wrote = range.start >= start && range.end <= new_end;
            if wrote {
                // One the edit wrote (pasted, a line moved) joining text
                // that was there; not a string the edit wrote whole and
                // new, nor one the text before held so at that place.
                let trimmed = holder.text.trim();
                let new = written.contains(trimmed) && !before.contains(trimmed);
                let kept = earlier.is_some_and(|earlier| {
                    field_value(earlier, &holder.pointer).and_then(Value::as_str)
                        == Some(holder.text)
                });
                return (beside && !new && !kept).then(|| holder.pointer.clone());
            }
            // Outside the edit (one it cut into is the edit's), where it
            // was before it.
            if range.end > start && range.start < new_end {
                return None;
            }
            let was = if range.end <= start {
                range.clone()
            } else {
                let byte = range.start - new_end + old_end;
                byte..byte + token.len()
            };
            if earlier.is_none() {
                // A repair: text beside it the host did not write so.
                return (beside && !self.host_wrote(before, &holder.pointer, holder.text))
                    .then(|| holder.pointer.clone());
            }
            // A holder before that no reading places stood for a token
            // given whole: alone.
            let was = was_held(&was);
            if was.as_ref().is_some_and(|was| !self.alone(was.text, token)) {
                return None;
            }
            // Alone before: text beside it now, or a list or map made of
            // the value holding it (the token put in brackets, a child
            // given to its element).
            let repetition = behavior.repetition();
            let made =
                was.is_some_and(
                    |was| match relation(&holder.pointer, &was.pointer, repetition) {
                        Some(Relation::Inside) => true,
                        Some(Relation::Copy | Relation::Item) => repetition != Repetition::Elements,
                        _ => false,
                    },
                );
            (beside || made).then(|| holder.pointer.clone())
        })
    }
}
