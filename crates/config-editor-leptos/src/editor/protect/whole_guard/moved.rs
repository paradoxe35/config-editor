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
use super::super::tokens::field_value;
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

/// The strings inside `value` (at `pointer`) holding `token`, with their
/// pointers.
fn strings_holding<'v>(
    value: &'v Value,
    pointer: &str,
    token: &str,
    found: &mut Vec<(String, &'v str)>,
) {
    match value {
        Value::String(text) if text.contains(token) => found.push((pointer.to_string(), text)),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                strings_holding(item, &format!("{pointer}/{index}"), token, found);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                strings_holding(item, &format!("{pointer}/{key}"), token, found);
            }
        }
        _ => {}
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
        // One the edit wrote (pasted, a line moved) joining text that was
        // there (a secret item's line pasted under `port: 1` reads
        // `1 - «…»`): glued. A value the edit wrote whole is its own (a
        // mask pasted into a note).
        let written = &after[start..new_end];
        // Only tokens on lines the edit wrote: a line reading as the text
        // before's line of the same number is kept (a Replace All rewrites
        // the text between two changes, every token there with it); a line
        // moved or pasted reads otherwise there.
        let before_lines: Vec<&str> = before.split('\n').collect();
        let mut kept = Vec::with_capacity(near.len());
        let (mut line, mut byte) = (0, 0);
        for (range, _) in &near {
            line += after[byte..range.start].matches('\n').count();
            byte = range.start;
            let line_start = after[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let line_end = after[range.end..]
                .find('\n')
                .map_or(after.len(), |at| range.end + at);
            kept.push(before_lines.get(line) == Some(&&after[line_start..line_end]));
        }
        let joined = with_spans(behavior, after, &[], || {
            near.iter()
                .zip(&kept)
                .filter(|((range, _), kept)| {
                    !**kept && range.start >= start && range.end <= new_end
                })
                .find_map(|((range, units), _)| {
                    let token = &after[range.clone()];
                    let pointer = path_at(behavior, after, *units)
                        .map_or_else(String::new, |path| path.to_pointer());
                    let value = field_value(document, &pointer)?;
                    let text = value.as_str()?;
                    (self.index.in_field(token, &pointer).is_none()
                        && held(Some(value), token) == Held::Glued
                        && !written.contains(text.trim()))
                    .then_some(pointer)
                })
        });
        if joined.is_some() {
            return joined;
        }
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
                    let pointer = path_at(behavior, after, *units)
                        .map_or_else(String::new, |path| path.to_pointer());
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
        // It may only have had text beside it already, as the text before
        // reads. After a repair (it does not parse), a token the repair
        // leaves read with other text in one string (a list's key line
        // made a scalar running over its items) is glued; one in a list or
        // map there is plain text, flagged.
        let earlier = super::super::super::parse_cache::parsed(behavior, before);
        let Some(earlier) = earlier.as_ref().as_ref().ok() else {
            // Each string holding it there (the path read where it is
            // written may be a parent's: a line at the start of a plain
            // scalar written over lines).
            return glued.into_iter().find_map(|(token, pointer, _)| {
                let mut strings = Vec::new();
                strings_holding(
                    field_value(document, &pointer)?,
                    &pointer,
                    token,
                    &mut strings,
                );
                strings.into_iter().find_map(|(at, text)| {
                    (text.trim() != token && !self.host_wrote(before, &at, text)).then_some(at)
                })
            });
        };
        with_spans(behavior, before, &[], || {
            glued.into_iter().find_map(|(token, pointer, (_, units))| {
                let was = path_at(behavior, before, units)
                    .map_or_else(String::new, |path| path.to_pointer());
                (held(field_value(earlier, &was), token) != Held::Glued).then_some(pointer)
            })
        })
    }
}
