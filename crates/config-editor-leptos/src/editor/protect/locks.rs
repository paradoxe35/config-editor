//! The locks of one text: every read-only field (from the schema and the
//! host's paths) and every read-only range, located in the text on screen.

use std::cell::OnceCell;
use std::ops::Range;
use std::sync::Arc;

use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::super::paths::{FieldPath, key_range};
use super::carry::Moves;
use super::locate::{LineIndex, extent, lookup, within};
use super::marks::{SchemaMarks, schema_marks};
use super::scan;
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
    let (document, error_line) = match codec::read(behavior, text, sources.reading_schema) {
        Ok(document) => (Some(document), None),
        Err(error) => (None, error.line),
    };
    let carried = sources.previous.filter(|_| document.is_none());
    let marks = match (sources.marking_schema, &document) {
        (Some(schema), Some(document)) => schema_marks(document, schema),
        _ => SchemaMarks::default(),
    };
    // A text that does not parse, where a lock cannot be carried from the
    // text before (there is none, or the lock is new): the locks of the
    // same text with the line breaking it blanked, when that parses, with
    // the values read there; else the fields written in it, line by line.
    let standing = OnceCell::new();
    let stand_in = || {
        standing
            .get_or_init(|| {
                if document.is_some() {
                    return None;
                }
                scan::stand_in(behavior, text, error_line, sources.reading_schema).map(|stand| {
                    let protected = protect(
                        behavior,
                        &stand.text,
                        Sources {
                            ranges: &[],
                            previous: None,
                            ..sources
                        },
                    );
                    (protected, stand.blanked)
                })
            })
            .as_ref()
    };
    let scanned = OnceCell::new();
    let written = || {
        scanned
            .get_or_init(|| scan::written(behavior, text, &lines, None))
            .as_deref()
    };
    // Too large to read line by line, with no stand-in: what it holds is
    // not known, so it is held as a field not found (below).
    let mut unread = false;
    let marked: Vec<(String, Reason)> = match (carried, &document, sources.marking_schema) {
        (Some(previous), _, _) => previous.marked.clone(),
        (None, None, Some(schema)) => {
            // The stand-in's marks, and those on the lines it blanked.
            let (mut marked, found) = match stand_in() {
                Some((stand, blanked)) => (
                    stand.marked.clone(),
                    scan::written(behavior, text, &lines, Some(blanked)),
                ),
                None => (Vec::new(), written().map(<[_]>::to_vec)),
            };
            unread = found.is_none();
            for (pointer, reason) in scan::marked(behavior, schema, &found.unwrap_or_default()) {
                if !marked.iter().any(|(known, _)| *known == pointer) {
                    marked.push((pointer, reason));
                }
            }
            marked
        }
        _ => marks
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
    let moves = carried.map(|previous| Moves::new(&previous.text, text));
    let mut pins: Vec<(String, Value)> = carried
        .map(|previous| previous.pins.clone())
        .unwrap_or_default();
    for (pointer, reason) in outer {
        // A text that does not parse: the lock of the text before, moved
        // through the edit, when the edit kept its text.
        let moved = carried.zip(moves.as_ref()).and_then(|(previous, moves)| {
            let lock = previous
                .locks
                .iter()
                .find(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))?;
            moves.lock(lock)
        });
        if let Some(lock) = moved {
            protected.locks.push(Lock { reason, ..lock });
            continue;
        }
        let pinned = pins.iter().any(|(known, _)| *known == pointer);
        // Else, in a text that does not parse, the stand-in's lock (with
        // its value pinned), unless a value was pinned in a text before.
        let standing = (document.is_none() && !pinned)
            .then(stand_in)
            .flatten()
            .and_then(|(stand, _)| {
                let lock = stand
                    .locks
                    .iter()
                    .find(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))?;
                let pin = stand.pins.iter().find(|(known, _)| *known == pointer)?;
                Some((lock.clone(), pin.clone()))
            });
        if let Some((lock, pin)) = standing {
            protected.locks.push(Lock { reason, ..lock });
            pins.push(pin);
            continue;
        }
        // Else located in the text, with what is inside it as the parsed
        // text (or the value pinned in the last one that parsed) has it.
        let value = match &document {
            Some(document) => lookup(document, &pointer),
            None => pins
                .iter()
                .find(|(pinned, _)| *pinned == pointer)
                .map(|(_, value)| value),
        };
        // Without a value to say what is inside it, the lines written
        // inside it.
        let inside = match (value, &document) {
            (None, None) => Some(written().unwrap_or_default()),
            _ => None,
        };
        let found = extent(behavior, text, &lines, value, inside, &pointer).or_else(|| {
            // Not found where its path says (a bracket left open above it
            // moves every line below into the list): the lines written
            // under its own key wherever they seem to be, so an unreadable
            // text locks more rather than less.
            inside
                .and_then(|written| scan::under_own_key(behavior, text, &lines, written, &pointer))
        });
        let Some(found) = found else {
            // Written, by the last text that parsed or the stand-in (or
            // nothing says otherwise), but not found: held until the text
            // parses.
            if document.is_none() {
                let has = |document: &Value| lookup(document, &pointer).is_some();
                let last = carried.and_then(|previous| previous.parsed.as_deref());
                let stand = stand_in().and_then(|(stand, _)| stand.document.as_deref());
                // Found, or held so, in the text before.
                let before = carried.is_some_and(|previous| {
                    previous.unplaced.iter().any(|(known, _)| *known == pointer)
                        || previous
                            .locks
                            .iter()
                            .any(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))
                });
                let written = before
                    || match (last, stand) {
                        (None, None) => true,
                        (last, stand) => last.is_some_and(has) || stand.is_some_and(has),
                    };
                if written {
                    protected.unplaced.push((pointer, reason));
                }
            }
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
    protected.pins = match &document {
        None => pins,
        Some(document) => protected
            .locks
            .iter()
            .filter_map(|lock| {
                let pointer = lock.pointer.as_ref()?;
                Some((pointer.clone(), lookup(document, pointer)?.clone()))
            })
            .collect(),
    };
    protected.secret = match (carried, &document) {
        (Some(previous), _) => previous.secret.clone(),
        (None, None) => stand_in()
            .map(|(stand, _)| stand.secret.clone())
            .unwrap_or_default(),
        (None, Some(_)) => marks.secret,
    };
    if unread {
        protected.unplaced.push((String::new(), Reason::Field));
    }
    protected.marked = marked;
    protected.document = document.map(Arc::new);
    protected.parsed = match &protected.document {
        Some(document) => Some(Arc::clone(document)),
        None => carried.and_then(|previous| previous.parsed.clone()),
    };
    protected.text = text.to_string();
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
