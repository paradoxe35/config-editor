//! A locked block (a field whose pinned value is a map or a list) is text
//! from its key to its value's end: a line written between its key and
//! its first entry, or between two of its entries, adds to it even where
//! the value's own stretches keep their text (a TOML table's first key, a
//! YAML block's first child). Whitespace there is formatting and free, and
//! so is any edit after which the text parses with the block as pinned (a
//! comment in it);
//! text written right after the block's last stretch is past it (a key
//! added after a block with no closer: see `locks` for the save waiting).

use super::super::locate::lookup;
use super::super::locks::Lock;
use super::edit::Edited;
use super::stretch::is_blank;

/// Whether `edit` writes or removes more than whitespace inside `lock`'s
/// block, between its key's end and its value's end.
pub(super) fn written_inside(edit: &Edited<'_>, lock: &Lock) -> bool {
    let Some(pointer) = lock.pointer.as_deref() else {
        return false;
    };
    let Some((_, pinned)) = edit
        .protected
        .pins
        .iter()
        .find(|(known, _)| known == pointer)
    else {
        return false;
    };
    let container = pinned.is_object() || pinned.is_array();
    let (Some(first), Some(last)) = (
        lock.ranges.iter().map(|range| range.start).min(),
        lock.ranges.iter().map(|range| range.end).max(),
    ) else {
        return false;
    };
    if !container {
        return false;
    }
    let start = lock.key.as_ref().map_or(first, |key| key.end.min(first));
    let region = &edit.region;
    let inside =
        region.start >= start && region.start < last || region.start < start && region.end > start;
    let code = |units: &[u16]| units.iter().any(|unit| !is_blank(*unit));
    let written = code(&edit.old[region.clone()]) || code(&edit.new[edit.new_region.clone()]);
    // A comment changed in it, the block reading as before: free.
    let kept = || {
        edit.parsed()
            .is_some_and(|edited| lookup(edited, pointer) == Some(pinned))
    };
    // Its text left whole by an edit around it (Replace All on both sides).
    let whole = || {
        let from = lock.key.as_ref().map_or(first, |key| key.start.min(first));
        let block = String::from_utf16_lossy(&edit.old[from..last]);
        edit.after.contains(&block)
    };
    inside && written && !kept() && !whole()
}
