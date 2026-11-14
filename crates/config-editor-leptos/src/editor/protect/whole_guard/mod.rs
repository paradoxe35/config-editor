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
//!   beside it may only lose some; after a repair (the text before did
//!   not parse) the token must stand alone;
//! - else (the edited text does not parse) the field is held as a read-only
//!   field is through a syntax error, by the lock check (`guard`): its
//!   value's text is kept, nothing glued on its ends, no line joining it or
//!   indented under it; its key stays free to rename or move (the value
//!   moves with it). An edit taking the placeholder away whole (a
//!   secret replaced) is free. Read near the placeholders only: an edit
//!   elsewhere cannot write beside one, and the repair is checked when the
//!   text parses (`broken`). Each placeholder near the edit is matched to
//!   its field by where it is written, never paired with every field
//!   holding the same token (one fixed mask in thousands of fields).
//!
//! With no placeholder given whole, an edit costs the check nothing; with
//! some, an edit out of their reach costs lookups among them (`index`),
//! one in reach a read of the text and work for each field given one,
//! never a pass of the text, or of the fields, for each.

mod broken;
mod index;

pub(crate) use index::WholeIndex;

use std::ops::Range;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::glued::{Holding, holding, only_taken_from};
use super::reach::reaches_none_by;
use super::tokens::Known;

/// The host's placeholders in the text before an edit.
pub(crate) struct Placeholders<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) reading_schema: Option<&'a Value>,
    pub(crate) known: &'a [Known],
    /// `known`'s tokens given whole, looked up (built once for them).
    pub(crate) index: &'a WholeIndex,
    /// UTF-16 ranges of the real placeholders in the text before, in order.
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
        if self.index.is_empty() || before == after {
            return Verdict {
                glued: None,
                parses: None,
            };
        }
        // Out of reach: no placeholder on the edited line, and its path is
        // no field given one, nor inside or holding one; a lookup among
        // them, never a pass over them. (Such an edit keeps its line's
        // keys, so it makes no placeholder real again either.)
        if before_parses
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
                glued: self.glued_in_parsed(before, document),
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
    /// made a list or map stays as it was; after a repair (`before` does
    /// not parse) it holds the token alone, as the host wrote it or bare.
    fn glued_in_parsed(&self, before: &str, document: &Value) -> Option<String> {
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
                    (_, Holding::Alone(_)) => false,
                    (Some(Holding::Beside(was)), Holding::Beside(now)) => {
                        !only_taken_from(was, now)
                    }
                    (Some(Holding::Within(was)), Holding::Within(now)) => was != now,
                    _ => true,
                }
            })
            .and_then(|known| known.field.clone())
    }
}
