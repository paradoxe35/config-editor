//! Whether an edit keeps every lock: one check for every way text changes
//! (typing, paste, cut, delete, replace, completions, Revert, comment
//! toggling, undo), given the text before and after. What is protected is
//! a set of spans of the text (see `locks`): each locked field's value, its
//! own key and the keys holding it, and the host's ranges, placed from the
//! last text that parsed and followed through edits by position.
//!
//! An edit changing one scalar on a line no lock is on, in a text that
//! parses, is kept without reading the document (`reach`). Any other is
//! refused when
//! - it changes a span's text (a locked value, a key holding one, a range);
//! - it glues text onto a span's ends (`port: 80` → `port: 808`,
//!   `xport: 80`); whitespace and punctuation next to it are free;
//! - it writes or removes more than whitespace inside a locked block,
//!   between its key and its value's end (`block`: a line above a TOML
//!   table's first key is the table's);
//!
//! A field of a host text that never parsed (no value pinned) may be
//! closed by the edit making the text parse (`closing`).
//!
//! Everything else is taken, whether the text parses or not: a key typed
//! beside a locked one whatever its name, a half-typed line, a duplicate
//! of a locked key or block, a locked line moved or commented out, a key
//! added past a block's end. No keystroke is refused for how the parse
//! reads the locked value: when it reads otherwise (a duplicate shadows
//! it, it moved under another parent), the lock is held (see
//! `locks::Protected::held`): its text stays locked, the save, the change
//! event and the value wait with the `saveHeld` reason, until the text
//! reads it as pinned again. A field the language cannot locate (it gives
//! no spans) has its value for a span: an edit whose text parses with
//! another value is refused, and while the text does not parse, one taking
//! away its value's text (as written) is.
//!
//! Formatting inside a locked span stays free: whitespace between its
//! tokens (a space after `:` or `=`, re-indenting where indentation carries
//! no structure: JSON, TOML, HCL, XML), and lines moved whole, while the
//! text parses and every locked value reads as pinned; while it does not
//! parse, whitespace between tokens (`spacing`: a space inside a string or
//! a YAML plain value excepted). Where indentation is structure (YAML) a
//! locked block's inner indents cannot change; nor can a locked value's
//! content written over lines or as XML text, whose whitespace is content
//! too (`content`).

mod block;
mod closing;
mod content;
mod edit;
mod pins;
mod spacing;
mod stretch;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::locks::{Lock, Protected};
use super::reach::out_of_reach;
use super::texts::Reason;
use crate::EditorStrings;
use block::written_inside;
use closing::only_closes;
use content::{glued_to_content, written_string_changed};
use edit::Edited;
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

/// Checks the edit turning `before` (whose locks are `protected`) into
/// `after`, read off the two texts alone (see [`check_at`]).
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

/// Checks the edit turning `before` (whose locks are `protected`) into
/// `after`, the caret it left at `caret` (UTF-16) when known: every span
/// kept, and, for a field the language cannot locate, its value.
pub(crate) fn check_at(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    protected: &Protected,
    (before, after): (&str, &str),
    caret: Option<usize>,
) -> Result<(), Blocked> {
    if protected.is_empty() || before == after {
        return Ok(());
    }
    // One scalar changed off every lock's lines: nothing to read.
    if out_of_reach(behavior, protected, before, after).is_some() {
        return Ok(());
    }
    let edit = Edited::new(behavior, reading_schema, protected, before, after, caret);
    spans_kept(&edit)?;
    if !protected.unplaced.is_empty() {
        let reason = match edit.parsed() {
            Some(edited) => unplaced_hold(edited, protected),
            None => unplaced_text_cut(protected, before, after),
        };
        if let Some(reason) = reason {
            return Err(refused(reason));
        }
    }
    Ok(())
}

/// Every lock's spans survive the edit: its value's stretches and its own
/// key, then the keys holding it; whitespace changed inside a locked field
/// is free when every locked value still reads as pinned, and so are its
/// lines moved whole past several others (a reorder made at once).
fn spans_kept(edit: &Edited<'_>) -> Result<(), Blocked> {
    for lock in &edit.protected.locks {
        // A field a host range covers keeps its value's text; its key is
        // the range's text where the range covers it (a parse of a line
        // half typed above it may read the new text into its key).
        if lock.pointer.is_some() && lock.reason == Reason::Range {
            covered_value_kept(edit, lock)?;
            continue;
        }
        let free = || {
            lock.pointer.is_some()
                && (edit.reformat_only()
                    || edit.spacing_only()
                    || edit.moved_whole(lock)
                    || only_closes(edit, lock))
        };
        if !lock
            .ranges
            .iter()
            .chain(&lock.key)
            .all(|range| edit.kept(range))
            && !free()
        {
            return Err(refused(&lock.reason));
        }
        if written_inside(edit, lock) && !free() {
            return Err(refused(&lock.reason));
        }
        if glued_to_content(edit, lock)
            || (edit.parsed().is_none() && written_string_changed(edit, lock))
        {
            return Err(refused(&lock.reason));
        }
        let spans: Vec<_> = lock
            .ranges
            .iter()
            .chain(&lock.key)
            .chain(&lock.guards)
            .cloned()
            .collect();
        let held =
            || lock.guards.iter().all(|range| edit.kept(range)) && edit.kept_together(&spans);
        if !held() && !free() {
            return Err(Blocked {
                reason: lock.reason.clone(),
                why: Why::Holds,
            });
        }
    }
    Ok(())
}

/// The value stretches of a field a host range covers (past its key) are
/// kept, or the edit only re-flows whitespace or moves lines whole.
fn covered_value_kept(edit: &Edited<'_>, lock: &Lock) -> Result<(), Blocked> {
    let value = |range: &std::ops::Range<usize>| match &lock.key {
        Some(key) if key.start <= range.start && range.start < key.end => key.end..range.end,
        _ => range.clone(),
    };
    let kept = lock.ranges.iter().all(|range| edit.kept(&value(range)));
    let free = || edit.reformat_only() || edit.spacing_only() || edit.moved_whole(lock);
    if !kept && !free() || written_inside(edit, lock) && !free() {
        return Err(refused(&lock.reason));
    }
    Ok(())
}

/// The edit refused for changing a lock held for `reason`.
fn refused(reason: &Reason) -> Blocked {
    Blocked {
        reason: reason.clone(),
        why: Why::Locked,
    }
}
