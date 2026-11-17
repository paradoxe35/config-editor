//! Placeholders written where no field the host gave them is: one fixed
//! mask whose field's parent key was renamed (which of the alike fields it
//! was cannot be told), or whose line was moved into another block. Such a
//! token is plain text there (and flagged), yet it is still the host's
//! token standing for a whole secret: nothing may be written beside it, or
//! the host would hear `«…»MY` as a new value. They are found by their text
//! near the edit (a few lines each way, never a pass of the document), and
//! each is judged at the path it is written at now, before and after the
//! edit, by the strings there holding it (a language may name only a
//! container holding it, the root at worst): a value that held it alone
//! may not get text beside it, nor become a list or map holding it; one
//! that already had text beside it is the user's (a mask pasted into a
//! note). Alone is the token as is, or in its host field's form.

use std::cell::OnceCell;
use std::ops::Range;

use serde_json::Value;

use super::super::super::paths::{path_at, with_spans};
use super::super::super::text::text_size;
use super::super::super::text_change::utf16_len;
use super::super::tokens::field_value;
use super::Placeholders;

/// Non-blank lines each way from an edit read for tokens: a multi-line
/// form's header or closing line, the lines a value runs over.
const LINES_AROUND: usize = 3;

/// A token written in a text: its byte range, and its UTF-16 start.
pub(super) type Written = (Range<usize>, usize);

fn holds(value: &Value, token: &str) -> bool {
    match value {
        Value::String(text) => text.contains(token),
        Value::Array(items) => items.iter().any(|item| holds(item, token)),
        Value::Object(map) => map.values().any(|item| holds(item, token)),
        _ => false,
    }
}

/// The path `token` is read at in `document`: where the text writes it
/// (`path`, the root when none), cut back to a value of the document that
/// holds it. The text's reading of a path may name a field the parse does
/// not have (a list's items read where the parse makes them one scalar:
/// the lines still say `/keys/1`, the document holds `/keys`), or one
/// beside the token's (a quote the language declares that its parser
/// reads as text, `user:" 'app'; password: '«…»'`): the parse knows which
/// value holds the token.
fn read_at(
    document: &Value,
    path: Option<super::super::super::paths::FieldPath>,
    token: &str,
) -> String {
    let mut pointer = path.map_or_else(String::new, |path| path.to_pointer());
    while !pointer.is_empty()
        && !field_value(document, &pointer).is_some_and(|value| holds(value, token))
    {
        pointer.truncate(pointer.rfind('/').unwrap_or(0));
    }
    pointer
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
                let key = crate::editor::language::pointer_key(key);
                strings_holding(item, &format!("{pointer}/{key}"), token, found);
            }
        }
        _ => {}
    }
}

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

/// The longest pointer both `one` and `other` start with, segment by
/// segment.
fn shared_pointer(one: &str, other: &str) -> String {
    let mut shared = String::new();
    for (mine, theirs) in one.split('/').zip(other.split('/')).skip(1) {
        if mine != theirs {
            break;
        }
        shared = format!("{shared}/{mine}");
    }
    shared
}

impl Placeholders<'_> {
    /// The strings inside the value at `pointer` of `document` holding
    /// `token` with other text, with their pointers; not those at a field
    /// given that token (judged there, see `glued_in_parsed`).
    fn beside<'v>(
        &self,
        document: &'v Value,
        pointer: &str,
        token: &str,
    ) -> Vec<(String, &'v str)> {
        let mut strings = Vec::new();
        if let Some(value) = field_value(document, pointer) {
            strings_holding(value, pointer, token, &mut strings);
        }
        strings.retain(|(at, text)| {
            !self.alone(text, token) && self.index.in_field(token, at).is_none()
        });
        strings
    }

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

    /// Whether the value at `pointer` of `document`, no field given `token`,
    /// is a list or map holding it.
    fn made_container(&self, document: &Value, pointer: &str, token: &str) -> bool {
        self.index.in_field(token, pointer).is_none()
            && field_value(document, pointer)
                .is_some_and(|value| (value.is_array() || value.is_object()) && holds(value, token))
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
        let parsed_before = OnceCell::new();
        let earlier = || {
            parsed_before
                .get_or_init(|| super::super::super::parse_cache::parsed(behavior, before))
                .as_ref()
                .as_ref()
                .ok()
        };
        // One the edit wrote (pasted, a line moved) joining text that was
        // there (a secret item's line pasted under `port: 1` reads
        // `1 - «…»`): glued. A value the edit wrote whole, new to the text,
        // is its own (a mask pasted into a note); one the text before had
        // (its line moved) is judged as any.
        let written = after.get(start..new_end).unwrap_or_default();
        // Only tokens on lines the edit wrote: a line reading as the text
        // before's line of the same number is kept (a Replace All rewrites
        // the text between two changes, every token there with it); a line
        // moved or pasted reads otherwise there.
        let before_lines: Vec<&str> = before.split('\n').collect();
        let mut kept = Vec::with_capacity(near.len());
        let (mut line, mut byte) = (0, 0);
        for (range, _) in &near {
            line += after
                .get(byte..range.start)
                .unwrap_or_default()
                .matches('\n')
                .count();
            byte = range.start;
            let line_start = after
                .get(..range.start)
                .unwrap_or_default()
                .rfind('\n')
                .map_or(0, |at| at + 1);
            let line_end = after
                .get(range.end..)
                .unwrap_or_default()
                .find('\n')
                .map_or(after.len(), |at| range.end + at);
            kept.push(
                before_lines.get(line)
                    == Some(&after.get(line_start..line_end).unwrap_or_default()),
            );
        }
        let joined = with_spans(behavior, after, &[], || {
            near.iter()
                .zip(&kept)
                .filter(|((range, _), kept)| {
                    !**kept && range.start >= start && range.end <= new_end
                })
                .find_map(|((range, units), _)| {
                    let token = after.get(range.clone()).unwrap_or_default();
                    let pointer = read_at(document, path_at(behavior, after, *units), token);
                    // The strings there holding it with text beside it, but
                    // one the edit wrote whole and new, or one the text
                    // before held so at the same place (the user's note).
                    self.beside(document, &pointer, token)
                        .into_iter()
                        .find(|(at, text)| {
                            (!written.contains(text.trim()) || before.contains(text.trim()))
                                && earlier().is_none_or(|earlier| {
                                    field_value(earlier, at).and_then(Value::as_str) != Some(text)
                                })
                        })
                        .map(|(at, _)| at)
                })
        });
        if joined.is_some() {
            return joined;
        }
        let removed = utf16_len(before.get(start..old_end).unwrap_or_default());
        let inserted = utf16_len(after.get(start..new_end).unwrap_or_default());
        // Glued now: a string holding it with text beside it at a path no
        // field given its token is, or the value it is read at, at no such
        // field, a list or map holding it. With where it was (its bytes and
        // UTF-16 start in `before`).
        let glued: Vec<(&str, String, Written)> = with_spans(behavior, after, &[], || {
            near.iter()
                // One the edit wrote (pasted, a line moved) is new there.
                .filter(|(range, _)| range.end <= start || range.start >= new_end)
                .filter_map(|(range, units)| {
                    let token = after.get(range.clone()).unwrap_or_default();
                    let pointer = read_at(document, path_at(behavior, after, *units), token);
                    if self.beside(document, &pointer, token).is_empty()
                        && !self.made_container(document, &pointer, token)
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
        let Some(earlier) = earlier() else {
            // Each string holding it there (the path read where it is
            // written may be a parent's: a line at the start of a plain
            // scalar written over lines).
            return glued.into_iter().find_map(|(token, pointer, _)| {
                self.beside(document, &pointer, token)
                    .into_iter()
                    .find_map(|(at, text)| (!self.host_wrote(before, &at, text)).then_some(at))
            });
        };
        // Judged where both paths meet (a language reading a path from
        // where text is written may name only a container holding it, the
        // document's root at worst): more strings there holding it with
        // text beside it than before, or the value that held it alone made
        // a list or map.
        with_spans(behavior, before, &[], || {
            glued.into_iter().find_map(|(token, pointer, (_, units))| {
                let was = read_at(earlier, path_at(behavior, before, units), token);
                let shared = shared_pointer(&pointer, &was);
                let more = self.beside(document, &shared, token).len()
                    > self.beside(earlier, &shared, token).len();
                let made = self.made_container(document, &pointer, token)
                    && field_value(earlier, &was)
                        .and_then(Value::as_str)
                        .is_some_and(|text| self.alone(text, token));
                (more || made).then_some(pointer)
            })
        })
    }
}
