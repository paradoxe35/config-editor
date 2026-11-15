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
//!   break joined away (in an XML view) takes some, an XML element given a
//!   child or an attribute, or the token put in brackets, makes it a list
//!   or map, and each is refused. A field whose token the edit makes real
//!   again (its key renamed back, its line uncommented or moved back) is
//!   held to the same: the token alone. A value that already had text
//!   beside it may only lose some. After a repair (the text before did not
//!   parse) the token must stand alone: text that got beside it while the
//!   text was broken (not touching it: on a line below, in a block scalar)
//!   is taken out first;
//! - else (the edited text does not parse) each token written near the
//!   edit is a span: typing inside it or gluing text onto its edges is
//!   refused, taking it away whole is free (`broken`). Nothing is read off
//!   the broken text for fields: no key, copy or list is guessed.
//!
//! A token given whole written near the edit in none of its fields (its
//! field's parent key renamed, its line moved into another block) is held
//! the same way at the path it is at now (`moved`): plain text there, yet
//! still a whole secret's token.
//!
//! With no placeholder given whole, an edit costs the check nothing; with
//! some, an edit out of their reach costs lookups among them (`index`),
//! one in reach a read of the text and work for each field given one,
//! never a pass of the text, or of the fields, for each.

mod broken;
mod index;
mod moved;

pub(crate) use index::WholeIndex;

use std::ops::Range;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::text_change::changed_bytes;
use super::glued::{Holding, holding, only_taken_from};
use super::reach::reaches_none_by;
use super::tokens::{Known, SecretMatcher};

/// The host's placeholders in the text before an edit.
pub(crate) struct Placeholders<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) known: &'a [Known],
    /// `known`'s tokens given whole, looked up (built once for them).
    pub(crate) index: &'a WholeIndex,
    /// UTF-16 ranges of the real placeholders in the text before, in order.
    pub(crate) tokens: &'a [Range<usize>],
    /// The host's pattern, to find tokens near an edit that are in none of
    /// their fields (see `moved`).
    pub(crate) matcher: &'a SecretMatcher,
    /// The text from the host shown (when known): text beside a token it
    /// wrote is its own.
    pub(crate) origin: Option<&'a str>,
}

/// What the check found: the field of a placeholder the edit writes text
/// beside, and whether the edited text parses (`None`: not read).
pub(crate) struct Verdict {
    pub(crate) glued: Option<String>,
    pub(crate) parses: Option<bool>,
}

impl Placeholders<'_> {
    /// Whether the host wrote `now` as the value at `pointer`: its text's
    /// value there; a host text that does not parse holds it as written
    /// on one line (with none known, `before` does).
    pub(super) fn host_wrote(&self, before: &str, pointer: &str, now: &str) -> bool {
        let Some(origin) = self.origin else {
            return before.contains(now);
        };
        let parsed = parse_cache::parsed(self.behavior, origin);
        match parsed.as_ref() {
            Ok(document) => super::tokens::field_value(document, pointer)
                .and_then(Value::as_str)
                .is_some_and(|value| value == now),
            // As written, which says something of a value on one line only
            // (one over lines is written indented, or quoted).
            Err(_) => !now.trim().contains('\n') && origin.contains(now),
        }
    }

    /// The edit from `before` (which parses, when `before_parses`) to
    /// `after`.
    pub(crate) fn check(&self, before: &str, after: &str, before_parses: bool) -> Verdict {
        if self.index.is_empty() || before == after {
            return Verdict {
                glued: None,
                parses: None,
            };
        }
        let changed = changed_bytes(before, after);
        // Out of reach: no placeholder on the edited line, nor one given
        // whole written near it in none of its fields, and its path is
        // no field given one, nor inside or holding one; a lookup among
        // them, never a pass over them. (Such an edit keeps its line's
        // keys, so it makes no placeholder real again either.)
        if before_parses
            && self.moved_near(before, changed.0, changed.1).is_empty()
            && reaches_none_by(
                self.behavior,
                before,
                after,
                |lines| index::on_lines(self.tokens, lines),
                |path| self.index.reads(self.behavior, path),
            )
            .is_some()
        {
            return Verdict {
                glued: None,
                parses: Some(true),
            };
        }
        let parsed = parse_cache::parsed(self.behavior, after);
        match parsed.as_ref() {
            Ok(document) => Verdict {
                glued: self
                    .cut_into(before, after)
                    .or_else(|| self.glued_in_parsed(before, after, document))
                    .or_else(|| self.glued_where_moved(before, after, document, changed)),
                parses: Some(true),
            },
            Err(_) => Verdict {
                glued: self.held_while_broken(before, after),
                parses: Some(false),
            },
        }
    }

    /// The field of a placeholder given whole whose value the edited text
    /// (`document`) changes while still holding it: text beside it, the
    /// whitespace a multi-line form gives it (a `|` block's last line
    /// break, joined away in an XML view), or a list or map made of it (an
    /// XML element given a child or an attribute, the token put in
    /// brackets). Each field given one is judged, whether its token was
    /// real before the edit or is made real by it (a key renamed back,
    /// a line uncommented or moved back into its block). A value that
    /// already had text beside it may only lose some, and one already
    /// made a list or map stays as it was, and one made a list or map
    /// by an edit off the token's lines is flagged, not refused; after a
    /// repair (`before` does not parse) it holds the token alone.
    fn glued_in_parsed(&self, before: &str, after: &str, document: &Value) -> Option<String> {
        // Read the text before only for a value not as the host wrote it.
        let behavior = self.behavior;
        let mut changed = self
            .index
            .all()
            .map(|at| &self.known[at])
            .filter(|known| match holding(behavior, document, known) {
                Holding::Not => false,
                Holding::Alone(now) => known.whole.as_deref() != Some(now),
                Holding::Beside(_) | Holding::Within(_) => true,
            })
            .peekable();
        changed.peek()?;
        let earlier = parse_cache::parsed(behavior, before);
        let earlier = earlier.as_ref().as_ref().ok();
        changed
            .find(|known| {
                let was = earlier.map(|earlier| holding(behavior, earlier, known));
                match (was, holding(behavior, document, known)) {
                    // Bare where the host wrote whitespace around it (a
                    // block's last line break joined away): refused only
                    // where it held the host's form.
                    (Some(Holding::Alone(was)), Holding::Alone(now)) => was != now,
                    // A repair: the host's form, or the token bare; not
                    // another form (a kept block's last blank line moved
                    // away while the text was broken).
                    (None, Holding::Alone(now)) => {
                        known.whole.as_deref() != Some(now) && now != known.token
                    }
                    (_, Holding::Alone(_)) => false,
                    (Some(Holding::Beside(was)), Holding::Beside(now)) => {
                        !only_taken_from(was, now)
                    }
                    (Some(Holding::Within(was)), Holding::Within(now)) => was != now,
                    // Made a map (an attribute or a child given to its
                    // element): refused. Made a list: refused when the
                    // edit is on the token's own lines (brackets put
                    // around it); a sibling of its name written elsewhere
                    // makes it an item, which the parse flags and the save
                    // waits on.
                    (_, Holding::Within(Value::Array(_))) => {
                        on_token_lines(before, after, &known.token)
                    }
                    // A repair leaving the token with only whitespace around
                    // it: a form of it (a kept block's last blank line moved
                    // away), the host's or bare, as when alone.
                    (None, Holding::Beside(now)) if now.trim() == known.token => {
                        known.whole.as_deref() != Some(now) && now != known.token
                    }
                    // A repair: the token stands alone, or beside text the
                    // host wrote so (a copy of its key deleted is the way
                    // back); text that got beside it while the text was
                    // broken (a line typed under it) is taken out first.
                    (None, Holding::Beside(now)) => {
                        !self.host_wrote(before, known.field.as_deref().unwrap_or(""), now)
                    }
                    _ => true,
                }
            })
            .and_then(|known| known.field.clone())
    }
}

/// Whether the edit from `before` to `after` changes a line of `before`
/// writing `token`.
fn on_token_lines(before: &str, after: &str, token: &str) -> bool {
    let (start, old_end, _) = changed_bytes(before, after);
    before.match_indices(token).any(|(at, _)| {
        let line_start = before[..at].rfind('\n').map_or(0, |found| found + 1);
        let line_end = before[at..]
            .find('\n')
            .map_or(before.len(), |found| at + found);
        start <= line_end && line_start <= old_end
    })
}
