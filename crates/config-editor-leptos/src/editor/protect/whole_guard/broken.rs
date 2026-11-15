//! An edit leaving the text unparseable, near placeholders the host gave
//! whole: each token written near the edit (its own field's, or one moved
//! into another, found by the host's pattern) is a span of the text. The
//! edit is refused when it types inside one, or writes text (other than
//! whitespace) right at one of its edges. Replacing one (a cut into one is
//! taken whole), taking it away (its line cut or moved) and deleting text
//! beside one are free (but for whitespace taken from between it and other
//! text: a line joined onto it): when the text parses again, the parse
//! says where each token is (`problems`: a copy, another field's, glued),
//! and an edit making it parse with text beside a token is refused
//! (`glued_in_parsed`). No field is read off the broken text.

use super::super::super::paths::path_at;
use super::super::super::text_change::{changed_bytes, utf16_len};
use super::Placeholders;

/// Whether an edit replacing UTF-16 `first..last` changes part of the
/// token at `range` but not all of it: typing strictly inside it, or text
/// replaced over some of its characters. Text replaced over all of it
/// takes it away whole (a cut into one in its own field is widened to the
/// whole token first; the two texts may share some of its characters):
/// the parse decides where it went.
fn cuts_into(range: &std::ops::Range<usize>, first: usize, last: usize) -> bool {
    let typed_inside = first == last && range.start < first && first < range.end;
    let overlaps = first < range.end && range.start < last;
    let whole = first <= range.start && range.end <= last;
    typed_inside || overlaps && !whole
}

impl Placeholders<'_> {
    /// The field of a placeholder near the edit, its own field's or one
    /// moved into another (plain text there, still the host's token), that
    /// the edit changes part of (see [`cuts_into`]), whether the text
    /// parses after it or not.
    pub(super) fn cut_into(&self, before: &str, after: &str) -> Option<String> {
        let (start, old_end, _) = changed_bytes(before, after);
        let (first, last) = (utf16_len(&before[..start]), utf16_len(&before[..old_end]));
        self.written_near(before, start, old_end)
            .into_iter()
            .find_map(|(bytes, units)| {
                let token = &before[bytes];
                let range = units..units + utf16_len(token);
                (cuts_into(&range, first, last) && self.damaged(before, after, token))
                    .then(|| self.field_written(before, token, units))
            })
    }

    /// Whether the edit from `before` to `after`, cutting into `token`,
    /// damaged it: `after` holds it fewer times, and no token is written
    /// over the edited text (one pasted or undone over another shares
    /// some of its characters, which the diff of the texts keeps).
    fn damaged(&self, before: &str, after: &str, token: &str) -> bool {
        if after.matches(token).count() >= before.matches(token).count() {
            return false;
        }
        let (start, _, new_end) = changed_bytes(before, after);
        let line_start = after[..start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = after[new_end..]
            .find('\n')
            .map_or(after.len(), |at| new_end + at);
        !self
            .matcher
            .find_in(&after[line_start..line_end])
            .into_iter()
            .any(|found| line_start + found.start <= start && new_end <= line_start + found.end)
    }

    /// The field a token written at UTF-16 `units` of `text` is in, as
    /// written there, else its own.
    fn field_written(&self, text: &str, token: &str, units: usize) -> String {
        path_at(self.behavior, text, units)
            .map(|path| path.to_pointer())
            .or_else(|| {
                self.index
                    .first_of(token)
                    .and_then(|at| self.known[at].field.clone())
            })
            .unwrap_or_default()
    }

    /// The field of a placeholder near the edit that the edit, leaving the
    /// text unparseable, types inside or writes text right beside.
    pub(super) fn held_while_broken(&self, before: &str, after: &str) -> Option<String> {
        let (start, old_end, new_end) = changed_bytes(before, after);
        let near = self.written_near(before, start, old_end);
        if near.is_empty() {
            return None;
        }
        let inserted = &after[start..new_end];
        let (first, last) = (utf16_len(&before[..start]), utf16_len(&before[..old_end]));
        near.into_iter().find_map(|(bytes, units)| {
            let token = &before[bytes];
            let range = units..units + utf16_len(token);
            let inside = cuts_into(&range, first, last) && self.damaged(before, after, token);
            // Written at an edge: the inserted text's end against the
            // token's start, or its start against the token's end.
            let at_start = last == range.start && !inserted.ends_with(char::is_whitespace);
            let at_end = first == range.end && !inserted.starts_with(char::is_whitespace);
            // Or whitespace taken from between it and other text (a line
            // break after it deleted, joining the next line onto it).
            let removed = &before[start..old_end];
            let spacing = !removed.is_empty() && removed.chars().all(char::is_whitespace);
            let code = |character: Option<char>| character.is_some_and(|c| !c.is_whitespace());
            let joined = inserted.is_empty()
                && spacing
                && ((first == range.end && code(after[new_end..].chars().next()))
                    || (last == range.start && code(after[..start].chars().next_back())));
            let glued = (!inserted.is_empty() && (at_start || at_end)) || joined;
            if !inside && !glued {
                return None;
            }
            self.index.first_of(token)?;
            Some(self.field_written(before, token, range.start))
        })
    }
}
