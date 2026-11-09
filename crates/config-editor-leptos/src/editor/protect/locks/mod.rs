//! The locks of one text: every read-only field (from the schema and the
//! host's paths) and every read-only range, located in the text on screen.
//! `protect` runs the steps: `reading` (the text parsed, else read through
//! a stand-in or line by line), `fields` (which fields are protected) and
//! `place` (where each one is, or that it is held until the text parses).
//!
//! # A text that does not parse
//!
//! Two promises hold whatever order the text and the locks arrive in, at
//! any size and with any number of breaks:
//! - (I1) a locked value never changes in anything the editor sends
//!   (`on_change`, a save, `value()`);
//! - (I2) an edit leaving every locked value as it was is never refused:
//!   closing a bracket, quote or block, removing a stray line, a lock on a
//!   path the text never had, a large text.
//!
//! A locked value exists only in a text that parses, so the decision is
//! made at one moment: the edit that makes a text parse while the one
//! before did not (`guard::repair`). Each locked field then reads as its
//! *reference*, read from the *reference text* (`Protected::origin`: the
//! text the host gave, or the last one of it that parsed, or the text the
//! locks were first named on):
//! 1. the value pinned for it (the last text that parsed, or the stand-in:
//!    the reference text with its breaking lines blanked), equal;
//! 2. else, when the reference text parses, its value there, equal (both
//!    absent included);
//! 3. else, when the new text has the field: made there of the reference
//!    text's own lines, kept, with only closers added (`}`, `]`, `)`, end
//!    tags, a comma moved past them, and once the quote, `>` or `:`/`=`
//!    the line itself left out, its strings then all written there), none
//!    of its lines removed;
//! 4. else, absent from the new text, absent from the reference text too
//!    (no lock found there, no line of it removed, no key): a path the
//!    text never had keeps nothing, as in a text that parses.
//!
//! The fields checked are every lock, held field and pinned value of the
//! text before, the host's paths, and the fields the schema marks in the
//! new text and on the reference text's lines the edits removed: none
//! depends on finding the field in the broken text. A refusal names the
//! field (`Why::Changed`). The removed lines read for their paths are
//! those that may hold a protected field (naming one of the keys, or in the
//! block under such a line) and those where a field's own lines end, so
//! deleting unrelated lines costs nothing. One limit: when the reference
//! text never parsed and the edits since removed more lines naming a
//! protected key than can be read in a moment (`guard::repair::
//! REMOVED_READS`: about 80 in a text of 3000 lines), the repair is
//! refused, naming the field and asking to undo the deletion
//! (`Why::Unread`).
//!
//! While the text does not parse, the locks found in it (carried through
//! edits, from the stand-in, or by the line scan, which in a large text
//! reads only the lines naming a protected field) are shown and refuse an
//! edit on them at once; they never decide a value, so a lock not found
//! there costs only that early refusal.

mod fields;
mod place;
mod reading;

use std::ops::Range;
use std::sync::Arc;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::paths::{FieldPath, with_spans};
use super::texts::Reason;
use place::{Placed, Placer};
use reading::Reading;

/// One locked field or range of the text on screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lock {
    /// The field's JSON pointer; `None` for a range the host gave.
    pub(crate) pointer: Option<String>,
    /// The field's own key, which is shown as usual (only the value is
    /// muted).
    pub(crate) key: Option<Range<usize>>,
    /// UTF-16 ranges of the text the lock covers.
    pub(crate) ranges: Vec<Range<usize>>,
    /// The keys of the fields holding it: renaming one would move it.
    pub(crate) guards: Vec<Range<usize>>,
    /// Why it is read-only, shown on hover and when an edit is refused.
    pub(crate) reason: Reason,
}

impl Lock {
    /// Whether UTF-16 `offset` is inside the lock (its ends excluded).
    pub(crate) fn contains(&self, offset: usize) -> bool {
        self.ranges
            .iter()
            .any(|range| range.start < offset && offset < range.end)
    }
}

/// What protects one text: its locks, the document they were found in, and
/// its secret fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Protected {
    pub(crate) locks: Vec<Lock>,
    /// The text parsed (with the schema's array reading), when it parses and
    /// something needed it.
    pub(crate) document: Option<Arc<Value>>,
    /// The document of the last text that parsed, carried through texts
    /// that do not parse (within one text from the host).
    pub(crate) parsed: Option<Arc<Value>>,
    /// Locked fields written in this text that could not be found in it
    /// (see `Placed::Unplaced`): in a text that does not parse, checked, as
    /// every lock, when an edit makes the text parse; in one that parses (a
    /// language that cannot say where a field is written), pinned, so every
    /// edit whose text parses with another value is refused.
    pub(crate) unplaced: Vec<(String, Reason)>,
    /// Pointers of the secret fields written in the text.
    pub(crate) secret: Vec<String>,
    /// Each locked field's value, as the last text that parsed held it:
    /// carried through texts that do not parse (a key half typed), so an
    /// edit that makes the text parse again is compared with the value the
    /// lock was given, not with nothing.
    pub(crate) pins: Vec<(String, Value)>,
    /// The fields the schema marked read-only in the last text that parsed,
    /// with their reasons: still located while the text does not parse.
    pub(crate) marked: Vec<(String, Reason)>,
    /// The text the locks were found in, to move them through an edit that
    /// leaves the next text unparseable.
    pub(crate) text: String,
    /// Which text from the host the locks follow (see
    /// `ProtectState::trust_text`): pins are carried only within one.
    pub(crate) epoch: u64,
    /// The reference text: the one locked values are read from when no
    /// value was pinned (the text from the host, or the last that parsed),
    /// carried through texts that do not parse.
    pub(crate) origin: Option<Arc<str>>,
    /// The host's read-only paths, as pointers.
    pub(crate) paths: Vec<String>,
    /// Whether the schema marks fields read-only.
    pub(crate) marking: bool,
}

impl Protected {
    /// The lock covering UTF-16 `offset` (ends excluded), if any.
    pub(crate) fn lock_at(&self, offset: usize) -> Option<&Lock> {
        self.locks.iter().find(|lock| lock.contains(offset))
    }

    /// Whether nothing is locked.
    pub(crate) fn is_empty(&self) -> bool {
        self.locks.is_empty() && self.unplaced.is_empty()
    }

    /// Whether the text does not parse while fields may be locked: an edit
    /// making it parse is checked even when no lock was found in it.
    pub(crate) fn watches(&self) -> bool {
        self.document.is_none() && (self.marking || !self.paths.is_empty())
    }
}

/// Where the locks of a text come from.
#[derive(Clone, Copy, Default)]
pub(crate) struct Sources<'a> {
    /// The schema, when it marks fields (`readOnly`, secrets).
    pub(crate) marking_schema: Option<&'a Value>,
    /// The schema the text is read with (lone XML elements as arrays).
    pub(crate) reading_schema: Option<&'a Value>,
    /// Read-only fields the host named.
    pub(crate) paths: &'a [FieldPath],
    /// Read-only UTF-16 ranges of this text the host gave.
    pub(crate) ranges: &'a [Range<usize>],
    /// The locks of the text before, whose marks and pinned values carry
    /// over when this text does not parse.
    pub(crate) previous: Option<&'a Protected>,
}

/// The locks of `text`, written in `behavior`: the host's ranges, then each
/// protected field placed (see `place`), with the values to pin, the
/// secret fields, and what carries to the next text.
pub(crate) fn protect(
    behavior: &dyn ConfigLanguage,
    text: &str,
    sources: Sources<'_>,
) -> Protected {
    let mut protected = Protected {
        locks: range_locks(text, sources.ranges),
        ..Protected::default()
    };
    if sources.marking_schema.is_none() && sources.paths.is_empty() {
        return protected;
    }
    let reading = Reading::new(behavior, text, sources);
    let carried = sources.previous.filter(|_| reading.document.is_none());
    let marks = reading.schema_marks();
    let marked = fields::marked(&reading, carried, marks.read_only);
    let mut placer = Placer::new(&reading, carried);
    let outermost = fields::outermost(&reading, &marked);
    // Every field's spans read in one pass of the document, not each
    // field's in a pass of its own.
    let wanted = placer.spans_wanted(&outermost);
    with_spans(behavior, text, &wanted, || {
        for (pointer, reason) in outermost {
            match placer.place(pointer, reason) {
                Placed::Locked(lock) => protected.locks.push(lock),
                Placed::Unplaced(pointer, reason) => protected.unplaced.push((pointer, reason)),
                Placed::Absent => {}
            }
        }
    });
    protected.pins = placer.into_pins(&protected.locks, &protected.unplaced);
    protected.secret = fields::secret(&reading, carried, marks.secret);
    protected.marked = marked;
    protected.document = reading.into_document().map(Arc::new);
    protected.parsed = match &protected.document {
        Some(document) => Some(Arc::clone(document)),
        None => carried.and_then(|previous| previous.parsed.clone()),
    };
    protected.origin = match (&protected.document, carried) {
        (None, Some(previous)) if previous.origin.is_some() => previous.origin.clone(),
        _ => Some(Arc::from(text)),
    };
    protected.paths = sources.paths.iter().map(FieldPath::to_pointer).collect();
    protected.marking = sources.marking_schema.is_some();
    protected.text = text.to_string();
    protected
}

/// A lock for each of the host's `ranges` that is not empty, clamped to
/// the text.
fn range_locks(text: &str, ranges: &[Range<usize>]) -> Vec<Lock> {
    let total = if ranges.is_empty() {
        0
    } else {
        text.encode_utf16().count()
    };
    ranges
        .iter()
        .map(|range| range.start.min(total)..range.end.min(total))
        .filter(|range| range.start < range.end)
        .map(|range| Lock {
            pointer: None,
            key: None,
            ranges: vec![range],
            guards: Vec::new(),
            reason: Reason::Range,
        })
        .collect()
}
