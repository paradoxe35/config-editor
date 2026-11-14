//! A repair (the text before the edit does not parse) leaving a field
//! given its token whole with text beside it: refused, unless that text
//! was written beside it already in the broken text. A copy of the
//! field's key written twice (a line moved into a block holding it) is no
//! field of its own while the text does not parse; text written beside a
//! token there before the guard held it is the user's to take out after
//! the repair, and the repair itself (the other copy renamed or deleted)
//! is the way back, never refused.

use super::super::super::codec::scalar_range;
use super::super::super::paths::{path_at, written_value_span};
use super::super::super::text_change::utf16_len;
use super::super::glued::written_alone;
use super::super::own_value::own_value;
use super::super::tokens::Known;
use super::Placeholders;

impl Placeholders<'_> {
    /// Whether `known`'s token, beside other text in its field in `after`
    /// (which parses), was written so in `before` (which does not): its
    /// value's written text is written in `before` at the same field, in
    /// a value holding other text beside the token. (Found by its text,
    /// not by where the edit was: a line deleted next to its twin reads
    /// as either one deleted.)
    pub(super) fn glued_before_repair(&self, before: &str, after: &str, known: &Known) -> bool {
        let behavior = self.behavior;
        let Some(field) = known.field.as_deref() else {
            return false;
        };
        let Some(value) = written_value_span(behavior, after, field)
            .and_then(|span| scalar_range(behavior, after, span))
        else {
            return false;
        };
        let written = &after[value];
        let Some(offset) = written.find(&known.token) else {
            return false;
        };
        before.match_indices(written).any(|(at, _)| {
            let token = at + offset..at + offset + known.token.len();
            path_at(behavior, before, utf16_len(&before[..token.start]))
                .is_some_and(|path| path.to_pointer() == field)
                && written_alone(behavior, before, field, token.clone()).is_none()
                && own_value(behavior, before, field, &token).is_some()
        })
    }
}
