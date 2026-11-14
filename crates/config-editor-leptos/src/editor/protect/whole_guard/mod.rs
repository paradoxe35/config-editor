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
//!   text parses (`broken`).
//!
//! With no placeholder given whole, an edit costs the check nothing; with
//! some, one read of the text and work for each placeholder, never a pass
//! of the text for each.

mod broken;

use std::collections::HashMap;
use std::ops::Range;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::glued::{glued_in, only_taken_from, value_of};
use super::locks::Lock;
use super::reach::reaches_none;
use super::texts::Reason;
use super::tokens::Known;

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
        // Looked up by token: a pass over the host's tokens for each
        // placeholder was their count squared.
        let mut by_token: HashMap<&str, Vec<&Known>> = HashMap::new();
        for known in self.known {
            if known.whole.is_some() && known.field.is_some() {
                by_token
                    .entry(known.token.as_str())
                    .or_default()
                    .push(known);
            }
        }
        let units: Vec<u16> = before.encode_utf16().collect();
        self.tokens
            .iter()
            .flat_map(|range| {
                let written = String::from_utf16_lossy(
                    &units[range.start.min(units.len())..range.end.min(units.len())],
                );
                by_token
                    .get(written.as_str())
                    .into_iter()
                    .flatten()
                    .map(move |known| (range.clone(), *known))
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
}
