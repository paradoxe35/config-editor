//! What the edits on screen made of the host's last text, recorded edit by
//! edit while the host has not heard them (a save held). The host's ranges
//! name places in its text and are mapped through this record to the text
//! on screen, never guessed from the two texts.
//!
//! A drag is a move: the browser sends its delete and its drop as two
//! edits, which compose into a delete and an insert, so a place inside the
//! dragged text would map to nowhere. The record keeps the stretch a
//! drag's delete took (where it was in the host's text), and the drop that
//! writes that text whole records it as moved there: a range the host
//! then gives inside it lands on it where the drop put it.

use std::sync::Arc;

use super::super::edit::{Change, Edit, Moved, Origin};
use super::super::text::{TextRange, TextSize, slice};

/// The change the edits on screen made of the host's text `from` to give
/// the text on screen `to`.
#[derive(Clone)]
pub(super) struct SinceHost {
    pub(super) from: Arc<str>,
    pub(super) to: Arc<str>,
    pub(super) change: Change,
    /// The stretch of `from` the last edit, a drag's delete, took, and its
    /// text: the drop that comes next writes it elsewhere.
    dragged: Option<(TextRange, String)>,
}

impl SinceHost {
    /// The record with `edit` (made on `before`, making `after`) added to
    /// `since`, the record up to `before`; begun anew from `before` when
    /// that is the host's text (`since` is then not read).
    pub(super) fn with(
        since: Option<&SinceHost>,
        host_is_before: bool,
        edit: &Edit,
        before: &Arc<str>,
        after: &Arc<str>,
    ) -> Option<SinceHost> {
        let change = edit.mapping();
        let (from, earlier, composed) = if host_is_before {
            (before.clone(), None, change.clone())
        } else {
            let since = since.filter(|since| *since.to == **before)?;
            let composed = since.change.compose(&change)?.narrowed(&since.from)?;
            let composed = carried(composed, &since.change, &change);
            (since.from.clone(), Some(since), composed)
        };
        let kind = match &edit.origin {
            Origin::Input { kind, .. } => kind.as_str(),
            _ => "",
        };
        let mut dragged = None;
        let composed = match kind {
            "deleteByDrag" => {
                dragged = drag_taken(earlier, &from, before, &change);
                composed
            }
            "insertFromDrop" => match earlier.and_then(|since| since.dragged.as_ref()) {
                Some((stretch, text)) => dropped(composed, &change, *stretch, text),
                None => composed,
            },
            _ => composed,
        };
        Some(SinceHost {
            from,
            to: after.clone(),
            change: composed,
            dragged,
        })
    }
}

/// `composed` (`record` then `change`) with the stretches `record` moved
/// that `change` left whole, where `change` put them: composing keeps only
/// the replacements.
fn carried(composed: Change, record: &Change, change: &Change) -> Change {
    if record.moved().is_empty() {
        return composed;
    }
    let moved: Vec<Moved> = record
        .moved()
        .iter()
        .filter_map(|moved| {
            let went = change.map_range(&TextRange::at(moved.to, moved.len))?;
            (went.len() == moved.len).then_some(Moved {
                to: went.start(),
                ..*moved
            })
        })
        .collect();
    composed.with_moved(moved)
}

/// The stretch of `from` (the host's text) a drag's delete `change` (made
/// on `before`) took, and its text: `None` unless it took one stretch that
/// the edits before it (`earlier`, from `from` to `before`) left whole.
fn drag_taken(
    earlier: Option<&SinceHost>,
    from: &str,
    before: &str,
    change: &Change,
) -> Option<(TextRange, String)> {
    let mut edits = change.edits();
    let (taken, inserted) = edits.next()?;
    if edits.next().is_some() || !inserted.is_empty() || taken.is_empty() {
        return None;
    }
    let text = slice::slice(before, taken)?.to_string();
    let stretch = match earlier {
        None => taken,
        Some(since) => since.change.invert(from)?.map_range(&taken)?,
    };
    (slice::slice(from, stretch)? == text).then_some((stretch, text))
}

/// `composed` (from the host's text) with the drop `change` added, which
/// writes `text`, the stretch `stretch` of the host's text its drag took,
/// whole at one place: that stretch recorded as moved there.
fn dropped(composed: Change, change: &Change, stretch: TextRange, text: &str) -> Change {
    let mut edits = change.edits();
    let Some((at, inserted)) = edits.next() else {
        return composed;
    };
    if edits.next().is_some() || !at.is_empty() || inserted != text {
        return composed;
    }
    // Where the drop's text starts in the text after: the drop inserts at
    // `at` of the text before it, nothing before that place changed.
    let to = at.start();
    let moved = Moved {
        from: stretch.start(),
        len: TextSize::of(text),
        to,
    };
    let kept = composed.moved().to_vec();
    composed.with_moved(kept.into_iter().chain([moved]))
}
