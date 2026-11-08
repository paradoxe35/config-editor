//! The locks of one text: every read-only field (from the schema and the
//! host's paths) and every read-only range, located in the text on screen.
//! `protect` runs the steps: `reading` (the text parsed, else read through
//! a stand-in or line by line), `fields` (which fields are protected) and
//! `place` (where each one is, or that it is held until the text parses).

mod fields;
mod place;
mod reading;

use std::ops::Range;
use std::sync::Arc;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::paths::FieldPath;
use super::texts::Reason;
use fields::Marked;
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
    /// Locked fields written in this text (it does not parse) that could
    /// not be found in it: no edit making the text parse goes through
    /// while there is one, as their values could not be compared.
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
    let Marked {
        fields: marked,
        unread,
    } = fields::marked(&reading, carried, marks.read_only);
    let mut placer = Placer::new(&reading, carried);
    for (pointer, reason) in fields::outermost(&reading, &marked) {
        match placer.place(pointer, reason) {
            Placed::Locked(lock) => protected.locks.push(lock),
            Placed::Unplaced(pointer, reason) => protected.unplaced.push((pointer, reason)),
            Placed::Absent => {}
        }
    }
    protected.pins = placer.into_pins(&protected.locks);
    protected.secret = fields::secret(&reading, carried, marks.secret);
    if unread {
        protected.unplaced.push((String::new(), Reason::Field));
    }
    protected.marked = marked;
    protected.document = reading.into_document().map(Arc::new);
    protected.parsed = match &protected.document {
        Some(document) => Some(Arc::clone(document)),
        None => carried.and_then(|previous| previous.parsed.clone()),
    };
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
