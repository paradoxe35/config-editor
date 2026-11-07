//! The locks of one text: every read-only field (from the schema and the
//! host's paths) and every read-only range, located in the text on screen.

use std::ops::Range;

use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::super::paths::{FieldPath, key_range};
use super::locate::{LineIndex, extent, lookup, within};
use super::marks::{SchemaMarks, schema_marks};
use super::texts::Reason;

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
    pub(crate) document: Option<Value>,
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
        self.locks.is_empty()
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

/// The locks of `text`, written in `behavior`.
pub(crate) fn protect(
    behavior: &dyn ConfigLanguage,
    text: &str,
    sources: Sources<'_>,
) -> Protected {
    let mut protected = Protected::default();
    let total = if sources.ranges.is_empty() {
        0
    } else {
        text.encode_utf16().count()
    };
    for range in sources.ranges {
        let range = range.start.min(total)..range.end.min(total);
        if range.start < range.end {
            protected.locks.push(Lock {
                pointer: None,
                key: None,
                ranges: vec![range],
                guards: Vec::new(),
                reason: Reason::Range,
            });
        }
    }
    if sources.marking_schema.is_none() && sources.paths.is_empty() {
        return protected;
    }
    let lines = LineIndex::new(text);
    let document = codec::read(behavior, text, sources.reading_schema).ok();
    let carried = sources.previous.filter(|_| document.is_none());
    let marks = match (sources.marking_schema, &document) {
        (Some(schema), Some(document)) => schema_marks(document, schema),
        _ => SchemaMarks::default(),
    };
    let marked: Vec<(String, Reason)> = match carried {
        Some(previous) => previous.marked.clone(),
        None => marks
            .read_only
            .into_iter()
            .map(|(pointer, reason)| (pointer, reason.map_or(Reason::Field, Reason::Given)))
            .collect(),
    };
    let mut fields = marked.clone();
    for path in sources.paths {
        let pointer = path.to_pointer();
        let written = document
            .as_ref()
            .is_none_or(|document| lookup(document, &pointer).is_some());
        if written && !fields.iter().any(|(known, _)| *known == pointer) {
            fields.push((pointer, Reason::Field));
        }
    }
    // A field inside another locked one is covered by it.
    fields.sort();
    let outer: Vec<(String, Reason)> = fields
        .iter()
        .filter(|(pointer, _)| {
            !fields
                .iter()
                .any(|(other, _)| other != pointer && within(behavior, pointer, other))
        })
        .cloned()
        .collect();
    for (pointer, reason) in outer {
        let Some(found) = extent(behavior, text, &lines, document.as_ref(), &pointer) else {
            continue;
        };
        protected.locks.push(Lock {
            guards: ancestor_keys(behavior, text, &lines, &pointer),
            pointer: Some(pointer),
            key: found.key,
            ranges: found.ranges,
            reason,
        });
    }
    protected.pins = match (carried, &document) {
        (Some(previous), _) => previous.pins.clone(),
        (None, Some(document)) => protected
            .locks
            .iter()
            .filter_map(|lock| {
                let pointer = lock.pointer.as_ref()?;
                Some((pointer.clone(), lookup(document, pointer)?.clone()))
            })
            .collect(),
        (None, None) => Vec::new(),
    };
    protected.secret = match carried {
        Some(previous) => previous.secret.clone(),
        None => marks.secret,
    };
    protected.marked = marked;
    protected.document = document;
    protected
}

/// The keys of the fields holding `pointer`.
fn ancestor_keys(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    pointer: &str,
) -> Vec<Range<usize>> {
    let mut keys = Vec::new();
    let mut at = pointer;
    while let Some(cut) = at.rfind('/') {
        at = &at[..cut];
        if at.is_empty() {
            break;
        }
        if let Some(range) = key_range(behavior, text, at) {
            keys.push(lines.range(range));
        }
    }
    keys
}
