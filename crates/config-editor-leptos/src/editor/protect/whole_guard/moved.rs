//! Placeholders written where no field the host gave them is: one fixed
//! mask whose field's parent key was renamed (which of the alike fields it
//! was cannot be told), or whose line was moved into another block. Such a
//! token is plain text there (and flagged), yet it is still the host's
//! token standing for a whole secret: nothing may be written beside it, or
//! the host would hear `«…»MY` as a new value. They are found by their text
//! near the edit (a few lines each way, never a pass of the document), and
//! each is judged at the path it is written at now, before and after the
//! edit: a value that held it alone may not get text beside it, nor become
//! a list or map holding it; one that already had text beside it is the
//! user's (a mask pasted into a note).

use std::ops::Range;

use serde_json::Value;

use super::super::super::paths::{path_at, with_spans};
use super::super::super::text_change::utf16_len;
use super::super::glued::written_alone;
use super::super::own_value::with_own_values;
use super::super::tokens::{Known, field_value};
use super::Placeholders;

/// Non-blank lines each way from an edit read for tokens: a multi-line
/// form's header or closing line, the lines a value runs over.
const LINES_AROUND: usize = 3;

/// A token written in a text: its byte range, and its UTF-16 start.
pub(super) type Written = (Range<usize>, usize);

/// What a value holds of a token, wherever it is.
#[derive(PartialEq)]
enum Held {
    /// Not the token.
    Not,
    /// The token alone (whitespace around it a multi-line form writes).
    Alone,
    /// The token with something beside it: text, or the list or map the
    /// value became.
    Glued,
}

fn held(value: Option<&Value>, token: &str) -> Held {
    match value {
        Some(Value::String(text)) if text.trim() == token => Held::Alone,
        Some(Value::String(text)) if text.contains(token) => Held::Glued,
        Some(value @ (Value::Array(_) | Value::Object(_))) if holds(value, token) => Held::Glued,
        _ => Held::Not,
    }
}

fn holds(value: &Value, token: &str) -> bool {
    match value {
        Value::String(text) => text.contains(token),
        Value::Array(items) => items.iter().any(|item| holds(item, token)),
        Value::Object(map) => map.values().any(|item| holds(item, token)),
        _ => false,
    }
}

/// The bytes of `text` from [`LINES_AROUND`] non-blank lines above byte
/// `start` to as many below byte `end`.
fn around(text: &str, start: usize, end: usize) -> Range<usize> {
    let mut from = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let mut counted = 0;
    while counted < LINES_AROUND && from > 0 {
        let line_start = text[..from - 1].rfind('\n').map_or(0, |at| at + 1);
        if !text[line_start..from].trim().is_empty() {
            counted += 1;
        }
        from = line_start;
    }
    let mut to = text[end..].find('\n').map_or(text.len(), |at| end + at);
    let mut counted = 0;
    while counted < LINES_AROUND && to < text.len() {
        let line_end = text[to + 1..]
            .find('\n')
            .map_or(text.len(), |at| to + 1 + at);
        if !text[to + 1..line_end].trim().is_empty() {
            counted += 1;
        }
        to = line_end;
    }
    from..to
}

impl Placeholders<'_> {
    /// The tokens given whole written in `text` around bytes `start..end`,
    /// in order.
    pub(super) fn written_near(&self, text: &str, start: usize, end: usize) -> Vec<Written> {
        let window = around(text, start, end);
        let found = self.matcher.find_in(&text[window.clone()]);
        if found.is_empty() {
            return Vec::new();
        }
        let mut units = utf16_len(&text[..window.start]);
        let mut byte = window.start;
        let mut written = Vec::new();
        for range in found {
            let range = window.start + range.start..window.start + range.end;
            if self.index.first_of(&text[range.clone()]).is_none() {
                continue;
            }
            units += utf16_len(&text[byte..range.start]);
            byte = range.start;
            written.push((range, units));
        }
        written
    }

    /// [`Self::written_near`] in the text before the edit, but the real
    /// placeholders (those in the fields the host gave them).
    pub(super) fn moved_near(&self, before: &str, start: usize, end: usize) -> Vec<Written> {
        let mut written = self.written_near(before, start, end);
        written.retain(|(_, units)| {
            self.tokens
                .binary_search_by_key(units, |token| token.start)
                .is_err()
        });
        written
    }

    /// The field, at the path it is written at now, of a token given whole
    /// near the edit from `before` to `after` (which parses as `document`)
    /// that is in none of its token's fields, whose value held it alone
    /// and now has something beside it. `changed`: the bytes the edit
    /// replaced in `before`, and where the new ones end in `after`.
    pub(super) fn glued_where_moved(
        &self,
        before: &str,
        after: &str,
        document: &Value,
        (start, old_end, new_end): (usize, usize, usize),
    ) -> Option<String> {
        let near = self.written_near(after, start, new_end);
        if near.is_empty() {
            return None;
        }
        let behavior = self.behavior;
        let removed = utf16_len(&before[start..old_end]);
        let inserted = utf16_len(&after[start..new_end]);
        // Glued now, at a path no field given its token is: where it was
        // (its bytes and UTF-16 start in `before`).
        let glued: Vec<(&str, String, Written)> = with_spans(behavior, after, &[], || {
            near.iter()
                // One the edit wrote (pasted, a line moved) is new there.
                .filter(|(range, _)| range.end <= start || range.start >= new_end)
                .filter_map(|(range, units)| {
                    let token = &after[range.clone()];
                    let pointer = path_at(behavior, after, *units)?.to_pointer();
                    if self.index.in_field(token, &pointer).is_some()
                        || held(field_value(document, &pointer), token) != Held::Glued
                    {
                        return None;
                    }
                    let was = if range.end <= start {
                        (range.clone(), *units)
                    } else {
                        let byte = range.start - new_end + old_end;
                        (byte..byte + token.len(), *units + removed - inserted)
                    };
                    Some((token, pointer, was))
                })
                .collect()
        });
        if glued.is_empty() {
            return None;
        }
        // It may only have had text beside it already: as the text before
        // reads, or, when it does not parse (a repair), as it is written.
        let earlier = super::super::super::parse_cache::parsed(behavior, before);
        let earlier = earlier.as_ref().as_ref().ok();
        with_spans(behavior, before, &[], || {
            glued
                .into_iter()
                .find_map(|(token, pointer, (bytes, units))| {
                    let was = path_at(behavior, before, units)?.to_pointer();
                    let alone = match earlier {
                        Some(earlier) => held(field_value(earlier, &was), token) != Held::Glued,
                        None => written_alone(behavior, before, &was, bytes).is_some(),
                    };
                    alone.then_some(pointer)
                })
        })
    }

    /// The tokens given whole `moved` near the edit in `before` (which
    /// does not parse; see [`Self::moved_near`]), each with its UTF-16 range
    /// and a stand-in for its token given whole in the field it is
    /// written in, when that field's value is the token alone: held while
    /// the text is broken as a field's own token is. One with text beside
    /// it is the user's.
    pub(super) fn moved_alone(
        &self,
        before: &str,
        moved: Vec<Written>,
    ) -> Vec<(Range<usize>, Known)> {
        // Each one's field, then the values of those fields (every copy of
        // their keys) found at once.
        let placed: Vec<(Written, String)> = moved
            .into_iter()
            .filter_map(|(range, units)| {
                let pointer = path_at(self.behavior, before, units)?.to_pointer();
                Some(((range, units), pointer))
            })
            .collect();
        let pointers: Vec<String> = placed.iter().map(|(_, pointer)| pointer.clone()).collect();
        with_own_values(self.behavior, before, &pointers, || {
            placed
                .into_iter()
                .filter_map(|((range, units), pointer)| {
                    let token = &before[range.clone()];
                    let whole = written_alone(self.behavior, before, &pointer, range)?;
                    let ranges = units..units + utf16_len(token);
                    Some((
                        ranges,
                        Known {
                            token: token.to_string(),
                            field: Some(pointer),
                            count: 1,
                            whole: Some(whole),
                            items: Vec::new(),
                            placed: true,
                        },
                    ))
                })
                .collect()
        })
    }
}
