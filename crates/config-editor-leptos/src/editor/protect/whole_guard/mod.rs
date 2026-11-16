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

use super::super::text::TextRange;

use serde_json::Value;

use super::super::codec::scalar_range;
use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::paths::written_value_span;
use super::super::text_change::changed_bytes;
use super::glued::{Holding, holding, only_taken_from};
use super::reach::reaches_none_by;
use super::tokens::{Known, SecretMatcher, field_value};

/// The host's placeholders in the text before an edit.
pub(crate) struct Placeholders<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) known: &'a [Known],
    /// `known`'s tokens given whole, looked up (built once for them).
    pub(crate) index: &'a WholeIndex,
    /// The ranges of the real placeholders in the text before, in order.
    pub(crate) tokens: &'a [TextRange],
    /// The host's pattern, to find tokens near an edit that are in none of
    /// their fields (see `moved`).
    pub(crate) matcher: &'a SecretMatcher,
    /// The text from the host shown (when known): text beside a token it
    /// wrote is its own.
    pub(crate) origin: Option<&'a str>,
    /// Whether an edit was taken with a placeholder read otherwise (see
    /// [`Held`]) since the host's text: a repair of a text broken since
    /// is judged as an edit of a text that parses.
    pub(crate) holding: bool,
}

/// What the check found: the field of a placeholder the edit writes text
/// beside (refused), or that the parse reads with text beside it while the
/// edit left the placeholder and its edges alone (taken and held, see
/// [`Held`]), and whether the edited text parses (`None`: not read).
pub(crate) struct Verdict {
    pub(crate) glued: Option<String>,
    pub(crate) held: Option<Held>,
    pub(crate) parses: Option<bool>,
}

/// An edit taken although the parse reads a placeholder given whole with
/// text beside it (a key typed a character at a time before its entry,
/// `passwordx: password: '«…»'`): the placeholder's text and its edges are
/// as they were, so nothing on screen is glued; what reaches the host waits
/// until each of `tokens` reads in its fields as in the host's text again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    /// The field the parse reads the placeholder in, for the reason shown.
    pub(crate) field: String,
    /// The placeholders the edit left read otherwise.
    pub(crate) tokens: Vec<String>,
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
                held: None,
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
                held: None,
                parses: Some(true),
            };
        }
        let parsed = parse_cache::parsed(self.behavior, after);
        match parsed.as_ref() {
            Ok(document) => {
                if let Some(field) = self.cut_into(before, after) {
                    return Verdict {
                        glued: Some(field),
                        held: None,
                        parses: Some(true),
                    };
                }
                let read = self
                    .glued_in_parsed(before, after, document)
                    .or_else(|| self.glued_where_moved(before, after, document, changed));
                // Refused when the edit writes at a placeholder's edges (or
                // over it), or repairs the text (nothing before says what
                // its value held) but one held already; else the parse's
                // reading is held.
                let tokens = match &read {
                    Some(field) if before_parses || self.holding => {
                        self.untouched(before, after, changed, field)
                    }
                    _ => None,
                };
                match (read, tokens) {
                    (Some(field), Some(tokens)) => Verdict {
                        glued: None,
                        held: Some(Held { field, tokens }),
                        parses: Some(true),
                    },
                    (glued, _) => Verdict {
                        glued,
                        held: None,
                        parses: Some(true),
                    },
                }
            }
            Err(_) => Verdict {
                glued: self.held_while_broken(before, after),
                held: None,
                parses: Some(false),
            },
        }
    }

    /// When the edit (`changed`, see `changed_bytes`) writes neither over
    /// nor at the edges of any placeholder given whole, before or after it,
    /// the placeholders it leaves read otherwise: those written near it,
    /// and the token of the field read at `field`. `None` when it touches
    /// one. A placeholder's edges are those of the value its field writes
    /// (a block scalar's lines, a quoted string's quotes), as a locked
    /// value's: a line typed inside its block writes at it.
    fn untouched(
        &self,
        before: &str,
        after: &str,
        changed: (usize, usize, usize),
        field: &str,
    ) -> Option<Vec<String>> {
        // The edit as far as it may slide (`»</` to `»<z/></`: the same
        // text written right at the token).
        let (left, right) = slides(before, after, changed);
        let (start, old_end, new_end) = (changed.0 - left, changed.1 + right, changed.2 + right);
        let touches = |range: &Range<usize>, end: usize| range.start <= end && start <= range.end;
        let near_before = self.written_near(before, start, old_end);
        let near_after = self.written_near(after, start, new_end);
        if near_before.iter().any(|(range, _)| {
            touches(
                &self.written_extent(before, range).unwrap_or(range.clone()),
                old_end,
            )
        }) {
            return None;
        }
        for (range, _) in &near_after {
            // At the token, or inside the value its field writes now (a
            // line typed into its block).
            let now = self.written_extent(after, range).unwrap_or(range.clone());
            if touches(&now, new_end) {
                return None;
            }
            // Where it was: the edit is wholly before or after it.
            let was = if range.end < start {
                range.clone()
            } else {
                range.start - new_end + old_end..range.end - new_end + old_end
            };
            if touches(&self.written_extent(before, &was).unwrap_or(was), old_end) {
                return None;
            }
        }
        let mut tokens: Vec<String> = near_after
            .iter()
            .map(|(range, _)| after.get(range.clone()).unwrap_or_default().to_string())
            .chain(
                self.known
                    .iter()
                    .filter(|known| known.whole.is_some() && known.field.as_deref() == Some(field))
                    .map(|known| known.token.clone()),
            )
            .collect();
        tokens.sort();
        tokens.dedup();
        Some(tokens)
    }

    /// The bytes of `text` its field writes as its value, when they hold
    /// the placeholder at bytes `token`.
    fn written_extent(&self, text: &str, token: &Range<usize>) -> Option<Range<usize>> {
        let field = self
            .index
            .first_of(text.get(token.clone()).unwrap_or_default())
            .and_then(|at| self.known[at].field.as_deref())?;
        written_value_span(self.behavior, text, field)
            .and_then(|span| scalar_range(self.behavior, text, span))
            .filter(|range| range.start <= token.start && token.end <= range.end)
    }
}

impl Held {
    /// Whether `text` (written in `behavior`) still reads a token of the
    /// hold otherwise than the host's text `origin` does: it does not
    /// parse, a field the host gave one of them reads another value while
    /// the text holds the token, or more strings hold it with other text.
    pub(crate) fn still(
        &self,
        behavior: &dyn ConfigLanguage,
        known: &[Known],
        text: &str,
        origin: Option<&str>,
    ) -> bool {
        let parsed = parse_cache::parsed(behavior, text);
        let Ok(document) = parsed.as_ref() else {
            return true;
        };
        let host = origin.map(|origin| parse_cache::parsed(behavior, origin));
        let host = host.as_ref().and_then(|host| host.as_ref().as_ref().ok());
        self.tokens
            .iter()
            .filter(|token| text.contains(token.as_str()))
            .any(|token| {
                let mut fields = known
                    .iter()
                    .filter(|known| known.whole.is_some() && known.token == *token);
                let field_differs = fields.any(|known| {
                    let Some(field) = known.field.as_deref() else {
                        return false;
                    };
                    match host {
                        Some(host) => field_value(document, field) != field_value(host, field),
                        None => !matches!(holding(behavior, document, known), Holding::Alone(_)),
                    }
                });
                let beside = |value: &Value| beside_count(value, token);
                field_differs || host.map_or(0, beside) < beside(document)
            })
    }
}

/// How far (bytes left, bytes right) the edit `changed` (see
/// `changed_bytes`) from `before` to `after` could be written as well: an
/// insertion or deletion repeating the text beside it reads the same
/// anywhere along that run.
fn slides(
    before: &str,
    after: &str,
    (start, old_end, new_end): (usize, usize, usize),
) -> (usize, usize) {
    let (inserted, removed) = (
        after.get(start..new_end).unwrap_or_default(),
        before.get(start..old_end).unwrap_or_default(),
    );
    let along = |run: &mut dyn Iterator<Item = (char, char)>| -> usize {
        run.take_while(|(beside, edited)| beside == edited)
            .map(|(beside, _)| beside.len_utf8())
            .sum()
    };
    let slide = |edited: &str, left: bool| -> Option<usize> {
        (!edited.is_empty()).then(|| {
            if left {
                along(
                    &mut after
                        .get(..start)
                        .unwrap_or_default()
                        .chars()
                        .rev()
                        .zip(edited.chars().rev()),
                )
            } else {
                along(
                    &mut after
                        .get(new_end..)
                        .unwrap_or_default()
                        .chars()
                        .zip(edited.chars()),
                )
            }
        })
    };
    let both = |left: bool| match (slide(inserted, left), slide(removed, left)) {
        (Some(one), Some(other)) => one.min(other),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => 0,
    };
    (both(true), both(false))
}

/// How many strings inside `value` hold `token` with other text.
fn beside_count(value: &Value, token: &str) -> usize {
    match value {
        Value::String(text) => usize::from(text.contains(token) && text.trim() != token),
        Value::Array(items) => items.iter().map(|item| beside_count(item, token)).sum(),
        Value::Object(map) => map.values().map(|item| beside_count(item, token)).sum(),
        _ => 0,
    }
}

impl Placeholders<'_> {
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
        let line_start = before
            .get(..at)
            .unwrap_or_default()
            .rfind('\n')
            .map_or(0, |found| found + 1);
        let line_end = before
            .get(at..)
            .unwrap_or_default()
            .find('\n')
            .map_or(before.len(), |found| at + found);
        start <= line_end && line_start <= old_end
    })
}
