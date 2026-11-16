//! The locks of one text: every read-only field (from the schema and the
//! host's paths) and every read-only range, located in the text on screen.
//! `protect` runs the steps: `reading` (the text parsed, else read through
//! a stand-in or line by line), `fields` (which fields are protected) and
//! `place` (where each one is, or that it is held until the text parses).
//!
//! # Spans, followed by position
//!
//! The locks are spans of the text: each locked value, its own key and the
//! keys holding it, and the host's ranges. They are placed from a text that
//! parses (or, for a text from the host that never parsed, from its
//! stand-in: the text with its breaking lines blanked, else line by line),
//! and followed through every edit by position (`carry::Moves`): a text
//! that does not parse takes the locks of the text before, moved through
//! the edit. Nothing is read off a half-typed line.
//!
//! When the text parses again, each locked field is placed afresh where its
//! value reads as it was pinned. One that reads otherwise (a duplicate key
//! or block shadows it, it moved under another parent, it was commented
//! out) is *held* (`Protected::held`): it keeps the spans followed from the
//! text before and its pinned value, and the save waits until it reads as
//! pinned again (`live::ProtectState`'s save check). The edits that made it
//! so were let through: the guard refuses an edit only for the spans' text.

mod fields;
mod place;
mod reading;

use std::ops::Range;
use std::sync::Arc;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::paths::{FieldPath, with_spans};
use super::reach::out_of_reach;
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
    /// something needed it; for locks carried past an edit out of their
    /// reach (see `reach`), the text before it parsed: the same fields,
    /// every locked value as here, one scalar elsewhere read as it was.
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
    /// The locked fields whose value, in this text (which parses), reads
    /// otherwise than pinned: their spans are those followed from the text
    /// before, and a save waits until they read as pinned again.
    pub(crate) held: Vec<String>,
    /// The text from the host the locks follow (see `ProtectState::
    /// trust_text`): a lock named while the text does not parse takes its
    /// value from it, not from what was typed before the lock came.
    pub(crate) origin: Option<Arc<str>>,
    /// The host's read-only paths, as pointers.
    pub(crate) paths: Vec<String>,
    /// Whether the schema marks fields read-only.
    pub(crate) marking: bool,
    /// The host's read-only ranges the locks were found with (the fields
    /// they cover are locked by path too, see `fields::covered`).
    pub(crate) ranges: Vec<Range<usize>>,
    /// The fields the host's ranges cover in its own text, when this is a
    /// view in another language (see `Sources::range_fields`).
    pub(crate) range_fields: Vec<FieldPath>,
}

impl Protected {
    /// The lock covering UTF-16 `offset` (ends excluded), if any.
    pub(crate) fn lock_at(&self, offset: usize) -> Option<&Lock> {
        self.locks.iter().find(|lock| lock.contains(offset))
    }

    /// Why the field held at `pointer` is locked: its lock's reason, or,
    /// for one the text no longer has, its mark's (a path by default).
    pub(crate) fn held_reason(&self, pointer: &str) -> Reason {
        self.locks
            .iter()
            .find(|lock| lock.pointer.as_deref() == Some(pointer))
            .map(|lock| lock.reason.clone())
            .or_else(|| {
                self.marked
                    .iter()
                    .find(|(known, _)| known == pointer)
                    .map(|(_, reason)| reason.clone())
            })
            .unwrap_or(Reason::Field)
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
    /// The fields the host's ranges cover in its own text, locked here (a
    /// view in another language) as a range locks them: by their values.
    pub(crate) range_fields: &'a [FieldPath],
    /// The locks of the text before, whose marks and pinned values carry
    /// over when this text does not parse, and which move past an edit out
    /// of their reach (see `carried_past`).
    pub(crate) previous: Option<&'a Protected>,
    /// Where the edit from `previous`'s text to this one left its caret
    /// (UTF-16), when known: the locks follow the region it replaced as
    /// the guard placed it.
    pub(crate) caret: Option<usize>,
}

/// The locks of `text`, written in `behavior`: the host's ranges, then each
/// protected field placed (see `place`), with the values to pin, the
/// secret fields, and what carries to the next text.
pub(crate) fn protect(
    behavior: &dyn ConfigLanguage,
    text: &str,
    sources: Sources<'_>,
) -> Protected {
    if let Some(carried) = carried_past(behavior, text, sources) {
        return carried;
    }
    // The host's text the locks follow, kept while there are none yet: a
    // path named late takes its value from it.
    let origin = sources
        .previous
        .and_then(|previous| previous.origin.clone())
        .or_else(|| Some(Arc::from(text)));
    let mut protected = Protected {
        locks: range_locks(text, sources.ranges),
        origin,
        ..Protected::default()
    };
    if sources.marking_schema.is_none()
        && sources.paths.is_empty()
        && sources.range_fields.is_empty()
        && protected.locks.is_empty()
    {
        return protected;
    }
    let reading = Reading::new(behavior, text, sources);
    let carried = sources.previous.filter(|_| reading.document.is_none());
    let marks = reading.schema_marks();
    let mut marked = fields::marked(&reading, carried, marks.read_only);
    let mut covered_pins = Vec::new();
    let mut covered_fields = Vec::new();
    if carried.is_none() {
        let covered = fields::covered(&reading, &protected.locks);
        covered_fields = covered.fields;
        covered_pins = covered.pins;
    }
    let followed = sources
        .previous
        .filter(|previous| carried.is_some() || same_sources(previous, sources));
    let mut placer = Placer::new(&reading, followed);
    covered_fields.extend(
        sources
            .range_fields
            .iter()
            .map(|path| (path.to_pointer(), Reason::Range)),
    );
    placer.hold_unpinned();
    placer.release_covered(&covered_fields);
    let kept = placer.covered_before(&covered_fields, &protected.locks);
    marked.extend(covered_fields);
    marked.extend(kept);
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
        // A field held that is no longer marked or written where it is now
        // keeps its lock all the same.
        for (pointer, reason) in placer.held_left(&protected.locks) {
            if let Placed::Locked(lock) = placer.place(pointer, reason) {
                protected.locks.push(lock);
            }
        }
    });
    // A field locked two ways (a host range covering a path the host also
    // names, carried and placed again) is one lock: copies would pile up
    // with each text that does not parse.
    let mut seen = std::collections::HashSet::new();
    protected
        .locks
        .retain(|lock| seen.insert((lock.pointer.clone(), lock.ranges.clone(), lock.key.clone())));
    protected.held = placer.held_pointers(&protected.locks);
    protected.pins = placer.into_pins(&protected.locks, &protected.unplaced);
    for (pointer, value) in covered_pins {
        if !protected.pins.iter().any(|(known, _)| *known == pointer) {
            protected.pins.push((pointer, value));
        }
    }
    protected.secret = fields::secret(&reading, carried, marks.secret);
    // A field held stays marked: the texts after this one, parsing or not,
    // keep following it.
    for lock in &protected.locks {
        if let Some(pointer) = &lock.pointer
            && protected.held.contains(pointer)
            && !marked.iter().any(|(known, _)| known == pointer)
        {
            marked.push((pointer.clone(), lock.reason.clone()));
        }
    }
    protected.marked = marked;
    protected.document = reading.into_document().map(Arc::new);
    protected.parsed = match &protected.document {
        Some(document) => Some(Arc::clone(document)),
        None => carried.and_then(|previous| previous.parsed.clone()),
    };
    protected.paths = sources.paths.iter().map(FieldPath::to_pointer).collect();
    protected.marking = sources.marking_schema.is_some();
    protected.ranges = range_stretches(&protected.locks);
    protected.range_fields = sources.range_fields.to_vec();
    protected.text = text.to_string();
    protected
}

/// The locks of `text` made from those of the text before
/// (`sources.previous`, from the same sources) when the edit between them is
/// out of every lock's reach (see `reach`): moved past it, with the values
/// pinned and the fields marked there, which it left as they were. `None`
/// when it may reach one: the text is then read.
fn carried_past(
    behavior: &dyn ConfigLanguage,
    text: &str,
    sources: Sources<'_>,
) -> Option<Protected> {
    let previous = sources.previous?;
    // Back to the host's own text, nothing held is carried: it is read.
    let hosts_own = !previous.held.is_empty() && previous.origin.as_deref() == Some(text);
    if !same_sources(previous, sources) || previous.text == text || hosts_own {
        return None;
    }
    let shift = out_of_reach(behavior, previous, &previous.text, text)?;
    let mut locks = range_locks(text, sources.ranges);
    // The fields the ranges cover are those of the text before only when
    // the ranges are its own, moved past the edit.
    let moved: Vec<Range<usize>> = previous
        .ranges
        .iter()
        .map(|range| shift.moved_range(range))
        .collect();
    if moved != range_stretches(&locks) {
        return None;
    }
    locks.extend(
        previous
            .locks
            .iter()
            .filter(|lock| lock.pointer.is_some())
            .map(|lock| shift.moved(lock)),
    );
    Some(Protected {
        locks,
        document: previous.document.clone(),
        parsed: previous.parsed.clone(),
        unplaced: Vec::new(),
        secret: previous.secret.clone(),
        pins: previous.pins.clone(),
        marked: previous.marked.clone(),
        text: text.to_string(),
        epoch: previous.epoch,
        held: previous.held.clone(),
        origin: previous.origin.clone(),
        paths: previous.paths.clone(),
        marking: previous.marking,
        ranges: moved,
        range_fields: previous.range_fields.clone(),
    })
}

/// Whether `previous` was found with the same schema marking and host paths
/// as `sources` give.
fn same_sources(previous: &Protected, sources: Sources<'_>) -> bool {
    previous.marking == sources.marking_schema.is_some()
        && previous.paths.len() == sources.paths.len()
        && previous
            .paths
            .iter()
            .zip(sources.paths)
            .all(|(pointer, path)| *pointer == path.to_pointer())
}

/// The stretches of the host's ranges among `locks`.
fn range_stretches(locks: &[Lock]) -> Vec<Range<usize>> {
    locks
        .iter()
        .filter(|lock| lock.pointer.is_none())
        .flat_map(|lock| lock.ranges.iter().cloned())
        .collect()
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
