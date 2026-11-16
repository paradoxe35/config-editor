//! Whether an edit keeps every lock: one rule for every way text changes
//! (typing, paste, cut, delete, replace, completions, Revert, comment
//! toggling, undo), judged on the spans as the editor will hold them.
//!
//! What is protected is a set of spans of the text (see `locks`): each
//! locked field's value, its own key and the keys holding it, and the
//! host's ranges. The guard moves them through the edit with the code the
//! editor then holds them by: the host's ranges by `follow::follow_ranges`,
//! the locks by `locks::protect` given the locks before and the caret
//! (`carry::Moves` inside it, and the parse placing them when the text
//! parses). The edit is taken iff, so moved,
//! 1. every span holds the text it held (`spans`);
//! 2. no span has text glued onto its ends (`port: 80` → `port: 808`,
//!    `xport: 80`): whitespace and punctuation beside it are free;
//! 3. no locked field is lost (its lock, a hold or an unplaced field
//!    carries it).
//!
//! Then, in a text that parses, a locked value reading otherwise than
//! pinned (a duplicate shadows it, it moved under another parent, it was
//! commented out) is held (see `locks::Protected::held`): the save, the
//! change event and the value wait with the `saveHeld` reason until it
//! reads as pinned again. Nothing is refused for how the parse reads it.
//!
//! Formatting freedom comes from the followers: whitespace between spans
//! is no span's, lines moved whole carry their spans, and a span placed
//! again by its path reads as pinned when only whitespace inside it
//! changed (`spans`).
//!
//! A field the language cannot locate has no span: its value is checked
//! instead (`pins`), the one question here that is not a span question.
//!
//! An edit changing one scalar on a line no lock is on, in a text that
//! parses, is kept without moving anything (`reach`): `locks::protect`
//! moves the locks past it by its length, which keeps every span's text
//! and edges (`reach_tests` compares the two on random edits).

mod closing;
mod content;
mod pins;
mod spacing;
mod spans;
mod stretch;

use serde_json::Value;

use super::super::edit::{Change, CodeUnits};
use super::super::language::ConfigLanguage;
use super::super::paths::FieldPath;
use super::follow::follow_ranges;
use super::locks::{Protected, Sources, protect};
use super::reach::out_of_reach;
use super::texts::Reason;
use crate::EditorStrings;
use pins::{unplaced_hold, unplaced_text_cut};

/// An edit that would change a locked field, and why it is locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub(crate) reason: Reason,
    pub(crate) why: Why,
}

/// What the refused edit would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// Change a locked span's text.
    Locked,
    /// Rename or move a key holding the locked field.
    Holds,
}

impl Blocked {
    /// Why the edit is refused, as the tooltip says it.
    pub(crate) fn message(&self, strings: &EditorStrings) -> String {
        match self.why {
            Why::Holds => self.reason.holding(strings),
            Why::Locked => self.reason.text(strings),
        }
    }
}

/// Where the edit left its caret (UTF-16, in the text after), as the
/// followers take it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Carets<'a> {
    /// The caret the host's ranges follow by (`ProtectState::follow_edit`);
    /// unknown, where the edited region ends.
    pub(crate) caret: Option<usize>,
    /// Where the text the edit wrote ends, which the locks follow by
    /// (`Sources::caret`); `None` for an undo or redo.
    pub(crate) written: Option<usize>,
    /// What the edit recorded it did (`Edit::change`, in UTF-16), when it
    /// did: both followers move the spans through it.
    pub(crate) change: Option<&'a Change<CodeUnits>>,
}

/// Checks the edit turning `before` (whose locks are `protected`) into
/// `after`, read off the two texts alone (see [`check_at`]), with the
/// sources the locks before were found with.
#[cfg(test)]
pub(crate) fn check(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    protected: &Protected,
    before: &str,
    after: &str,
) -> Result<(), Blocked> {
    check_at(behavior, reading_schema, protected, (before, after), None)
}

/// [`check`], the caret left at `caret` (UTF-16) when known.
#[cfg(test)]
pub(crate) fn check_at(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    protected: &Protected,
    texts: (&str, &str),
    caret: Option<usize>,
) -> Result<(), Blocked> {
    let carets = Carets {
        caret,
        written: caret,
        change: None,
    };
    judge(behavior, reading_schema, protected, texts, carets).map(|_| ())
}

/// Judges the edit turning `before` (whose locks are `protected`) into
/// `after` by the one rule (see the module): `Ok` with the locks of the
/// text after as the editor will hold them (`None` when the edit is out
/// of every lock's reach and they only shift), else why it is refused.
///
/// The locks after are found from the sources the locks before were
/// (`Protected::paths`, `marking`, `ranges`, `range_fields`), as the
/// editor's memo of locks finds them.
pub(crate) fn judge(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    protected: &Protected,
    (before, after): (&str, &str),
    carets: Carets<'_>,
) -> Result<Option<Protected>, Blocked> {
    if protected.is_empty() || before == after {
        return Ok(None);
    }
    // One scalar changed off every lock's lines: nothing to move but by
    // its length (as `locks::protect` then does).
    if reach_allowed() && out_of_reach(behavior, protected, before, after).is_some() {
        return Ok(None);
    }
    let caret = carets.caret.unwrap_or_else(|| edited_end(before, after));
    let ranges = follow_ranges(&protected.ranges, before, after, caret, carets.change);
    let paths: Vec<FieldPath> = protected
        .paths
        .iter()
        .map(|pointer| FieldPath::from_pointer(pointer))
        .collect();
    let next = protect(
        behavior,
        after,
        Sources {
            marking_schema: reading_schema.filter(|_| protected.marking),
            reading_schema,
            paths: &paths,
            ranges: &ranges,
            range_fields: &protected.range_fields,
            previous: Some(protected),
            caret: carets.written,
            change: carets.change,
        },
    );
    spans::kept(
        behavior,
        protected,
        &next,
        (before, after),
        (&protected.ranges, &ranges),
        carets,
    )?;
    if !protected.unplaced.is_empty() {
        let reason = match &next.document {
            Some(edited) => unplaced_hold(edited, protected),
            None => unplaced_text_cut(protected, before, after),
        };
        if let Some(reason) = reason {
            return Err(refused(reason));
        }
    }
    Ok(Some(next))
}

/// Where the region the edit replaced ends in `after` (UTF-16), as narrow
/// as the texts allow: the caret of an edit that does not say.
fn edited_end(before: &str, after: &str) -> usize {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old.len() - prefix)
        .min(new.len() - prefix);
    new.len() - suffix
}

/// The edit refused for changing a lock held for `reason`.
fn refused(reason: &Reason) -> Blocked {
    Blocked {
        reason: reason.clone(),
        why: Why::Locked,
    }
}

#[cfg(test)]
thread_local! {
    static NO_REACH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the out-of-reach fast path may answer (test builds turn it off
/// to compare it with the full rule).
fn reach_allowed() -> bool {
    #[cfg(test)]
    {
        !NO_REACH.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// Runs `body` with the fast path off: every edit judged by the full rule.
#[cfg(test)]
pub(crate) fn without_reach<T>(body: impl FnOnce() -> T) -> T {
    NO_REACH.with(|off| off.set(true));
    let result = body();
    NO_REACH.with(|off| off.set(false));
    result
}
