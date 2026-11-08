//! Whether an edit keeps every lock: one check for every way text changes
//! (typing, paste, cut, delete, replace, completions, Revert, comment
//! toggling, undo), given the text before and after.
//!
//! An edit is refused when
//! - it changes a locked stretch's text, or a key holding a locked field;
//! - it glues text onto a locked stretch's ends (`port: 80` → `port: 808`,
//!   `xport: 80`); whitespace and punctuation next to it are free;
//! - the document still parses afterwards and a locked field's value there
//!   differs from the value pinned when the lock was last read (it moved
//!   under another parent, a comment took it, a line added below joined a
//!   locked block, a TOML table or dotted key added elsewhere extended it);
//! - the document does not parse afterwards and the edit adds a line that
//!   would join a locked field once it does: a line whose path is inside
//!   it, or a second key holding it (duplicates are errors here, but a
//!   host's parser may let the last one win).
//!
//! Formatting around a locked field stays free: blank lines, trailing
//! spaces, comments after the value, and whitespace inside it (a space
//! after `:` or `=`, re-indenting where indentation carries no structure:
//! JSON, TOML, HCL, XML) while the text parses and every locked value reads
//! as pinned. In YAML a locked line's indent cannot change. While the text
//! does not parse, a locked stretch keeps its text exactly, and a YAML
//! plain value takes nothing glued onto it (`abc<`).

mod edit;
mod joining;
mod pins;
mod stretch;

use std::ops::Range;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::carry::Moves;
use super::locate::LineIndex;
use super::locks::{Lock, Protected};
use super::texts::Reason;
use crate::EditorStrings;
use edit::Edited;
use joining::{is_near, joining_lines};
use pins::{is_pinned, pins_hold, reads_from_lock};
use stretch::indent_kept;

/// An edit that would change a locked field, and why it is locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub(crate) reason: Reason,
    /// It renames or moves a key holding the locked field.
    pub(crate) holds: bool,
}

impl Blocked {
    /// Why the edit is refused, as the tooltip says it.
    pub(crate) fn message(&self, strings: &EditorStrings) -> String {
        if self.holds {
            self.reason.holding(strings)
        } else {
            self.reason.text(strings)
        }
    }
}

/// Checks the edit turning `before` (whose locks are `protected`) into
/// `after`: each step below in turn, the first to refuse it saying why.
pub(crate) fn check(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    protected: &Protected,
    before: &str,
    after: &str,
) -> Result<(), Blocked> {
    if protected.is_empty() || before == after {
        return Ok(());
    }
    let edit = Edited::new(behavior, reading_schema, protected, before, after);
    stretches_kept(&edit)?;
    unplaced_held(&edit)?;
    let mut pointed: Vec<&Lock> = protected
        .locks
        .iter()
        .filter(|lock| lock.pointer.is_some())
        .collect();
    if pointed.is_empty() {
        return Ok(());
    }
    if let Some(edited) = edit.parsed() {
        pins_still_hold(edited, protected, &pointed)?;
        pointed.retain(|lock| !is_pinned(protected, lock));
        if pointed.is_empty() {
            return Ok(());
        }
        unpinned_read_from_locks(&edit, edited, &pointed)?;
    }
    nothing_joins(&edit, &pointed)?;
    indents_kept(&edit, &pointed)
}

/// Every lock's stretches, and the keys holding it, survive the edit;
/// whitespace changed inside a locked field is free when every locked
/// value still reads as pinned.
fn stretches_kept(edit: &Edited<'_>) -> Result<(), Blocked> {
    for lock in &edit.protected.locks {
        let free = || lock.pointer.is_some() && edit.reformat_only();
        if !lock.ranges.iter().all(|range| edit.kept(range)) && !free() {
            return Err(refused(&lock.reason));
        }
        if !lock.guards.iter().all(|range| edit.kept(range)) && !free() {
            return Err(Blocked {
                reason: lock.reason.clone(),
                holds: true,
            });
        }
    }
    Ok(())
}

/// A locked field not found in a text that does not parse: its value
/// cannot be compared, so the text stays unparsed until the host gives
/// one where it is found.
fn unplaced_held(edit: &Edited<'_>) -> Result<(), Blocked> {
    match edit.protected.unplaced.first() {
        Some((_, reason)) if edit.parsed().is_some() => Err(refused(reason)),
        _ => Ok(()),
    }
}

/// The edited text parses: every locked value must read as it was
/// pinned, wherever the edit was (a TOML `[web.sub]` table added at the
/// end changes `web`).
fn pins_still_hold(
    edited: &Value,
    protected: &Protected,
    pointed: &[&Lock],
) -> Result<(), Blocked> {
    let Some(pointer) = pins_hold(edited, protected) else {
        return Ok(());
    };
    let lock = pointed
        .iter()
        .find(|lock| lock.pointer.as_deref() == Some(pointer));
    Err(refused(
        &lock.map_or(Reason::Field, |lock| lock.reason.clone()),
    ))
}

/// A lock found in a text that never parsed has no value to compare with:
/// in the edited text, which parses, the field must read from the lock's
/// own text, moved through the edit and kept, and nothing else; and it is
/// held as every lock is while the text does not parse (the steps after).
fn unpinned_read_from_locks(
    edit: &Edited<'_>,
    edited: &Value,
    unpinned: &[&Lock],
) -> Result<(), Blocked> {
    let moves = Moves::new(edit.before, edit.after);
    let after_lines = LineIndex::new(edit.after);
    for lock in unpinned {
        if !reads_from_lock(
            edit.behavior,
            edit.after,
            &after_lines,
            edited,
            &moves,
            lock,
        ) {
            return Err(refused(&lock.reason));
        }
    }
    Ok(())
}

/// The values cannot be compared (the edited text does not parse, or the
/// lock has no pinned value). Nothing may be written where it would join
/// a locked field once it parses (a key half typed inside a locked block,
/// a duplicate of a locked key or of a key holding one): the edit may not
/// add a line whose path is inside a locked field, or which names a key
/// holding it.
fn nothing_joins(edit: &Edited<'_>, pointed: &[&Lock]) -> Result<(), Blocked> {
    let new_lines = LineIndex::new(edit.after);
    for lock in pointed {
        let pointer = lock.pointer.as_deref().unwrap_or_default();
        let count = |text: &str, lines: &LineIndex<'_>, region: &Range<usize>| {
            joining_lines(edit.behavior, text, lines, region, pointer)
        };
        let (Some(was), Some(now)) = (
            count(edit.before, &edit.lines, &edit.region),
            count(edit.after, &new_lines, &edit.new_region),
        ) else {
            return Err(refused(&lock.reason));
        };
        if now > was {
            return Err(refused(&lock.reason));
        }
    }
    Ok(())
}

/// Without a value to compare, a locked line near the edit keeps its
/// indent (in YAML it is structure) and whatever starts its line.
fn indents_kept(edit: &Edited<'_>, pointed: &[&Lock]) -> Result<(), Blocked> {
    for lock in pointed {
        let indent_kept = |range: &Range<usize>| {
            indent_kept(range, &edit.old, &edit.new, edit.prefix, edit.suffix)
                || edit.lines_kept(range)
        };
        if is_near(lock, &edit.region, &edit.lines) && !lock.ranges.iter().all(indent_kept) {
            return Err(refused(&lock.reason));
        }
    }
    Ok(())
}

/// The edit refused for changing a lock held for `reason`.
fn refused(reason: &Reason) -> Blocked {
    Blocked {
        reason: reason.clone(),
        holds: false,
    }
}
