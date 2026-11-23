//! An edit leaving the text unparseable, near placeholders the host gave
//! whole: each token written near the edit (its own field's, or one moved
//! into another, found by the host's pattern) is a span of the text. The
//! edit is refused when it types inside one, or writes text (other than
//! whitespace) right at one of its edges. Replacing one (a cut into one is
//! taken whole), taking it away (its line cut or moved) and deleting text
//! beside one are free (but for whitespace taken from between it and other
//! text: a line joined onto it). A token the edit writes (pasted, dropped,
//! typed to its last character) right against text already there is
//! refused as well, unless the language finds it written alone as the
//! value of the field it lands in. When the text parses again, the parse
//! says where each token is (`problems`: a copy, another field's, glued),
//! and an edit making it parse with text beside a token is refused
//! (`glued_in_parsed`). No field is read off the broken text.

use super::super::super::paths::path_at;
use super::super::super::text_change::changed_bytes;
use super::super::glued::written_alone;
use super::super::texts::shown_pointer;
use super::Placeholders;

/// Whether an edit replacing bytes `first..last` changes part of the
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
        let (first, last) = (start, old_end);
        self.written_near(before, start, old_end)
            .into_iter()
            .find_map(|(bytes, units)| {
                let token = before.get(bytes.clone()).unwrap_or_default();
                let range = bytes;
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
        let line_start = after
            .get(..start)
            .unwrap_or_default()
            .rfind('\n')
            .map_or(0, |at| at + 1);
        let line_end = after
            .get(new_end..)
            .unwrap_or_default()
            .find('\n')
            .map_or(after.len(), |at| new_end + at);
        !self
            .matcher
            .find_in(after.get(line_start..line_end).unwrap_or_default())
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
    /// text unparseable, types inside or writes text right beside, or
    /// writes right against other text.
    pub(super) fn held_while_broken(&self, before: &str, after: &str) -> Option<String> {
        let (start, old_end, new_end) = changed_bytes(before, after);
        self.written_at_token(before, after, (start, old_end, new_end))
            .or_else(|| self.written_against(before, after, (start, new_end)))
    }

    /// [`Self::held_while_broken`] for the tokens written in `before`.
    fn written_at_token(
        &self,
        before: &str,
        after: &str,
        (start, old_end, new_end): (usize, usize, usize),
    ) -> Option<String> {
        let near = self.written_near(before, start, old_end);
        if near.is_empty() {
            return None;
        }
        let inserted = after.get(start..new_end).unwrap_or_default();
        let (first, last) = (start, old_end);
        near.into_iter().find_map(|(bytes, units)| {
            let token = before.get(bytes.clone()).unwrap_or_default();
            let range = bytes;
            let inside = cuts_into(&range, first, last) && self.damaged(before, after, token);
            // Written at an edge: the inserted text's end against the
            // token's start, or its start against the token's end.
            let at_start = last == range.start && !inserted.ends_with(char::is_whitespace);
            let at_end = first == range.end && !inserted.starts_with(char::is_whitespace);
            // Or whitespace taken from between it and other text (a line
            // break after it deleted, joining the next line onto it).
            let removed = before.get(start..old_end).unwrap_or_default();
            let spacing = !removed.is_empty() && removed.chars().all(char::is_whitespace);
            let code = |character: Option<char>| character.is_some_and(|c| !c.is_whitespace());
            let joined = inserted.is_empty()
                && spacing
                && ((first == range.end
                    && code(after.get(new_end..).unwrap_or_default().chars().next()))
                    || (last == range.start
                        && code(after.get(..start).unwrap_or_default().chars().next_back())));
            let glued = (!inserted.is_empty() && (at_start || at_end)) || joined;
            if !inside && !glued {
                return None;
            }
            self.index.first_of(token)?;
            Some(shown_pointer(
                &self.field_written(before, token, units),
                &[token],
            ))
        })
    }

    /// The field of a token given whole that the edit (its new text at
    /// bytes `start..new_end` of `after`) writes, whole and new, right
    /// against text that was there (not whitespace or syntax), where the language
    /// does not find it written alone as the value of the field it lands
    /// in: `x` then the token pasted, or typed to its last character. The
    /// mirror of text written at a token's edge: the host would hear the
    /// token glued once the text parses, and no parse says so meanwhile.
    /// Text pasted with the token (its quotes, a key) is the paste's own;
    /// a token moved with its line keeps its neighbours.
    fn written_against(
        &self,
        before: &str,
        after: &str,
        (start, new_end): (usize, usize),
    ) -> Option<String> {
        if new_end == start {
            return None;
        }
        // Text a token is glued to: not whitespace, nor a character of the
        // language's syntax around a value (a quote, a bracket, a
        // separator, a comment marker), which the value's reading judges
        // once the text parses.
        let config = self.behavior.config();
        let syntax = |c: char| {
            let mut marks = config
                .quotes
                .iter()
                .chain(config.separators)
                .chain(config.line_comments)
                .copied()
                .chain(
                    config
                        .block_comment
                        .into_iter()
                        .flat_map(|(open, close)| [open, close]),
                );
            config
                .brackets
                .iter()
                .any(|&(open, close)| c == open || c == close)
                || config.item_separator == Some(c)
                || marks.any(|mark| mark.contains(c))
        };
        let code =
            |character: Option<char>| character.is_some_and(|c| !c.is_whitespace() && !syntax(c));
        self.written_near(after, start, new_end)
            .into_iter()
            .find_map(|(range, units)| {
                let token = after.get(range.clone()).unwrap_or_default();
                let written = range.start < new_end && start < range.end;
                // Against what was there: a neighbour outside the new text.
                let against = (range.start <= start
                    && code(
                        after
                            .get(..range.start)
                            .unwrap_or_default()
                            .chars()
                            .next_back(),
                    ))
                    || (new_end <= range.end
                        && code(after.get(range.end..).unwrap_or_default().chars().next()));
                if !written
                    || !against
                    || after.matches(token).count() <= before.matches(token).count()
                {
                    return None;
                }
                let field = self.field_written(after, token, units);
                // The refusal names the field: a broken text's key may hold
                // a piece of the token, which no message shows.
                written_alone(self.behavior, after, &field, range)
                    .is_none()
                    .then(|| shown_pointer(&field, &[token]))
            })
    }
}
