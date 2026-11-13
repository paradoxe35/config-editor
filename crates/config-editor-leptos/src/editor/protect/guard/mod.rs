//! Whether an edit keeps every lock: one check for every way text changes
//! (typing, paste, cut, delete, replace, completions, Revert, comment
//! toggling, undo), given the text before and after.
//!
//! An edit changing one scalar on a line no lock is on, in a text that
//! parses, is kept without reading the document (`reach`). Any other is
//! refused when
//! - it changes a locked stretch's text, or a key holding a locked field;
//! - it glues text onto a locked stretch's ends (`port: 80` → `port: 808`,
//!   `xport: 80`); whitespace and punctuation next to it are free;
//! - the document parses before and after, and a locked field's value
//!   differs from the value pinned when the lock was last read (it moved
//!   under another parent, a comment took it, a line added below joined a
//!   locked block, a TOML table or dotted key added elsewhere extended it);
//! - the document did not parse and now does, and a locked field does not
//!   read as its reference (`repair`, the rule in `locks`);
//! - the document does not parse afterwards and the edit adds a line that
//!   would join a locked field once it does: a line whose path is inside
//!   it, or a second key holding it (duplicates are errors here, but a
//!   host's parser may let the last one win).
//! - the document does not parse afterwards and the edit, made inside a
//!   locked block, opens a comment or string running over its lines below;
//! - the document does not parse afterwards, the language cannot say where
//!   a locked field is written, and the lines the edit changed, tried on
//!   the last text that parsed, change a pinned value (`spanless`).
//!
//! Formatting around a locked field stays free: blank lines, trailing
//! spaces, comments after the value, and whitespace inside it (a space
//! after `:` or `=`, re-indenting where indentation carries no structure:
//! JSON, TOML, HCL, XML) while the text parses and every locked value reads
//! as pinned. In YAML a locked line's indent cannot change. While the text
//! does not parse, a locked stretch keeps its text exactly but for
//! whitespace between its tokens (`spacing`: the same formatting, a space
//! inside a string or a YAML plain value excepted), a YAML plain value
//! takes nothing glued onto it (`abc<`); nor does a locked value's content
//! written over lines or as XML text, whose whitespace is content too
//! (`content`).

mod closers;
mod content;
mod edit;
mod hiding;
mod joining;
mod moved;
mod pins;
mod repair;
mod spacing;
mod spanless;
mod stretch;

use std::ops::Range;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::locate::LineIndex;
use super::locks::{Lock, Protected};
use super::reach::out_of_reach;
use super::texts::Reason;
use crate::EditorStrings;
use content::glued_to_content;
use edit::Edited;
use hiding::hidden;
use joining::{LinePaths, is_near, joining_lines, near_lines, span};
use moved::Place;
use pins::pins_hold;
use stretch::indent_kept;

/// An edit that would change a locked field, and why it is locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub(crate) reason: Reason,
    pub(crate) why: Why,
    /// The locked field, when the refusal names it.
    pub(crate) pointer: Option<String>,
}

/// What the refused edit would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// Change a locked field's text.
    Locked,
    /// Rename or move a key holding the locked field.
    Holds,
    /// Make the text parse with the locked field reading otherwise than in
    /// the host's text.
    Changed,
    /// Make the text parse after more changes to the host's text than can
    /// be checked.
    Unread,
}

impl Blocked {
    /// Why the edit is refused, as the tooltip says it.
    pub(crate) fn message(&self, strings: &EditorStrings) -> String {
        match (self.why, &self.pointer) {
            (Why::Holds, _) => self.reason.holding(strings),
            (Why::Changed, Some(pointer)) => self.reason.changed(strings, pointer),
            (Why::Unread, pointer) => self.reason.unchecked(strings, pointer.as_deref()),
            _ => self.reason.text(strings),
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
    if (protected.is_empty() && !protected.watches()) || before == after {
        return Ok(());
    }
    // One scalar changed off every lock's lines: nothing to read.
    if out_of_reach(behavior, protected, before, after).is_some() {
        return Ok(());
    }
    let edit = Edited::new(behavior, reading_schema, protected, before, after);
    // The edit making the text parse is decided by the repair rule alone,
    // which names the field: a closer added to a locked line completes it.
    let repairing = protected.document.is_none() && edit.parsed().is_some();
    stretches_kept(&edit, repairing)?;
    let mut pointed: Vec<&Lock> = protected
        .locks
        .iter()
        .filter(|lock| lock.pointer.is_some())
        .collect();
    if let Some(edited) = edit.parsed() {
        if protected.document.is_none() {
            return repair::repair_keeps_locks(&edit, edited);
        }
        pins_still_hold(edited, protected)?;
        pointed.retain(|lock| !edit.pinned().holds(lock));
    } else {
        spanless::hunks_keep_pins(&edit)?;
    }
    if pointed.is_empty() || edit.spacing_only() {
        return Ok(());
    }
    nothing_hidden(&edit, &pointed)?;
    nothing_joins(&edit, &pointed)?;
    indents_kept(&edit, &pointed)
}

/// Every lock's stretches, and the keys holding it, survive the edit;
/// whitespace changed inside a locked field is free when every locked
/// value still reads as pinned, and so are its lines moved whole past
/// several others (a reorder made at once). A repair checks the fields
/// itself.
fn stretches_kept(edit: &Edited<'_>, repairing: bool) -> Result<(), Blocked> {
    for lock in &edit.protected.locks {
        if repairing && lock.pointer.is_some() {
            continue;
        }
        let free = || {
            lock.pointer.is_some()
                && (edit.reformat_only() || edit.spacing_only() || edit.moved_whole(lock))
        };
        if !lock.ranges.iter().all(|range| edit.kept(range)) && !free() {
            return Err(refused(&lock.reason));
        }
        if glued_to_content(edit, lock) {
            return Err(refused(&lock.reason));
        }
        let held = || {
            lock.guards.iter().all(|range| edit.kept(range))
                && edit.kept_together(&[lock.ranges.as_slice(), &lock.guards].concat())
        };
        if !held() && !free() {
            return Err(Blocked {
                reason: lock.reason.clone(),
                why: Why::Holds,
                pointer: lock.pointer.clone(),
            });
        }
    }
    Ok(())
}

/// The edited text parses, as the text before did: every locked value must
/// read as it was pinned, wherever the edit was (a TOML `[web.sub]` table
/// added at the end changes `web`), the fields the language could not
/// locate included.
fn pins_still_hold(edited: &Value, protected: &Protected) -> Result<(), Blocked> {
    let Some(pointer) = pins_hold(edited, protected) else {
        return Ok(());
    };
    let reason = protected
        .locks
        .iter()
        .find(|lock| lock.pointer.as_deref() == Some(pointer))
        .map(|lock| &lock.reason)
        .or_else(|| {
            protected
                .unplaced
                .iter()
                .find(|(known, _)| known == pointer)
                .map(|(_, reason)| reason)
        });
    Err(refused(reason.unwrap_or(&Reason::Field)))
}

/// The values cannot be compared (the edited text does not parse, or the
/// lock has no pinned value). Nothing may be written where it would join
/// a locked field once it parses (a key half typed inside a locked block,
/// a duplicate of a locked key or of a key holding one): the edit may not
/// add a line whose path is inside a locked field, or which names a key
/// holding it; and a locked field the edit moves (a line command, a cut and
/// paste) stays where it is (see `moved`).
fn nothing_joins(edit: &Edited<'_>, pointed: &[&Lock]) -> Result<(), Blocked> {
    let new_lines = LineIndex::new(edit.after);
    // Each line's path is read once for all the locks.
    let old_paths = LinePaths::new(edit.behavior, edit.before, &edit.lines);
    let new_paths = LinePaths::new(edit.behavior, edit.after, &new_lines);
    let place = Place {
        edit,
        old: &old_paths,
        new: &new_paths,
        new_lines: &new_lines,
    };
    for lock in pointed {
        let pointer = lock.pointer.as_deref().unwrap_or_default();
        if is_near(lock, &edit.region, &edit.lines) && !place.kept(lock) {
            return Err(refused(&lock.reason));
        }
        let count = |paths: &LinePaths<'_>, region: &Range<usize>, near| {
            joining_lines(paths, region, near, pointer)
        };
        let Some((low, high)) = span(lock) else {
            continue;
        };
        let near_before = near_lines(edit.behavior, &edit.lines, low, high);
        // Where the lock is after the edit: before or after the region it
        // keeps its text; over it, anywhere in it.
        let near_after = if high <= edit.region.start {
            near_lines(edit.behavior, &new_lines, low, high)
        } else if low >= edit.region.end {
            let moved = |at: usize| at + edit.new.len() - edit.old.len();
            near_lines(edit.behavior, &new_lines, moved(low), moved(high))
        } else {
            0..new_lines.line_count()
        };
        let (Some(was), Some(now)) = (
            count(&old_paths, &edit.region, near_before),
            count(&new_paths, &edit.new_region, near_after),
        ) else {
            return Err(refused(&lock.reason));
        };
        if now > was {
            return Err(refused(&lock.reason));
        }
    }
    Ok(())
}

/// The edited text does not parse: an edit inside a locked block may not
/// open a comment or a string running over the block's lines below it
/// (`<!--` on a line of its own), where they would be text no lock is found
/// in. Opening one above a locked field, outside it, is typing as usual
/// (`<!--` before `-->`, a quote before its closer): the field is then
/// decided when the text parses again.
fn nothing_hidden(edit: &Edited<'_>, pointed: &[&Lock]) -> Result<(), Blocked> {
    if edit.parsed().is_some() {
        return Ok(());
    }
    // The locked text right after an edit made inside a locked stretch,
    // before and after the edit.
    let moved: Vec<(&Lock, usize, usize)> = pointed
        .iter()
        .flat_map(|lock| {
            lock.ranges
                .iter()
                .filter(|range| range.start < edit.region.start && edit.region.end < range.end)
                .map(|_| (*lock, edit.region.end, edit.new_region.end))
        })
        .collect();
    let starts = |pick: fn(&(&Lock, usize, usize)) -> usize| -> Vec<usize> {
        moved.iter().map(pick).collect()
    };
    let was = hidden(edit.behavior, edit.before, &starts(|(_, low, _)| *low));
    let now = hidden(edit.behavior, edit.after, &starts(|(_, _, after)| *after));
    for ((lock, _, _), (was, now)) in moved.iter().zip(was.into_iter().zip(now)) {
        if now && !was {
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
        why: Why::Locked,
        pointer: None,
    }
}
