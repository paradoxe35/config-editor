//! Where each locked field is in the text, tried in order: the lock of the
//! text before moved through the edit, when the edit kept its text (the
//! spans are followed by position, whether or not the text parses); else the stand-in's lock (its value pinned); else located by
//! its path (or under its own key); else kept as a field not found: in a
//! text that does not parse, until it parses; in one that parses (a
//! language that cannot locate fields), by its value alone, every edit then
//! compared with it (`guard`).

use std::ops::Range;

use serde_json::Value;

use super::super::super::codec;
use super::super::super::parse_cache;
use super::super::super::paths::key_range;
use super::super::carry::Moves;
use super::super::locate::{Extent, extent, inside, lookup, values_under};
use super::super::scan;
use super::super::texts::Reason;
use super::reading::Reading;
use super::{Lock, Protected};

/// Places the fields of one text, one after another, keeping the values
/// pinned so far.
pub(super) struct Placer<'r, 'a> {
    reading: &'r Reading<'a>,
    /// The locks of the text before, when this text does not parse.
    carried: Option<&'a Protected>,
    /// The locks of the text before, when this one parses: those of the
    /// fields held are followed from it.
    before: Option<&'a Protected>,
    moves: Option<Moves<'a>>,
    /// The values pinned in the texts before, then those of each stand-in
    /// lock placed.
    pins: Vec<(String, Value)>,
    /// The fields held, with their pinned values (see `Protected::held`).
    held: Vec<(String, Value)>,
    /// Whether the text is the host's own (the one the locks follow): it
    /// writes every field where the host did, so its paths place them.
    hosts_own: bool,
}

/// What became of one field.
pub(super) enum Placed {
    Locked(Lock),
    /// Written but not found: held until the text parses.
    Unplaced(String, Reason),
    /// Not written in the text.
    Absent,
}

impl<'r, 'a> Placer<'r, 'a> {
    /// Places the fields of `reading`'s text; `previous` is the text
    /// before's locks (from the same sources), when there is one.
    pub(super) fn new(reading: &'r Reading<'a>, previous: Option<&'a Protected>) -> Self {
        let carried = previous.filter(|_| reading.document.is_none());
        let before = previous.filter(|_| reading.document.is_some());
        // The host's own text reads every field as the host gave it: nothing
        // is held there (a field locked only while an edit covered it
        // with a range, gone again, included).
        let hosts_own = previous
            .and_then(|previous| previous.origin.as_deref())
            .is_some_and(|origin| origin == reading.text);
        // Held through a text that does not parse, as pinned.
        let held = match (before, carried, &reading.document) {
            _ if hosts_own => Vec::new(),
            (Some(before), _, Some(document)) => {
                let mut held = held_pins(before, document);
                for item in items_read_fewer(before, document) {
                    if !held.iter().any(|(pointer, _)| *pointer == item.0) {
                        held.push(item);
                    }
                }
                // A field named by its path reading as in the host's text
                // (both without it included) reaches the host unchanged: a
                // value pinned in a text the user made (a cut put a block's
                // key at the root) is not the host's. A field a host range
                // covers is no path: its list index moves with items added.
                let by_path = |pointer: &str| {
                    before.locks.iter().any(|lock| {
                        lock.pointer.as_deref() == Some(pointer) && lock.reason != Reason::Range
                    })
                };
                if held.iter().any(|(pointer, _)| by_path(pointer))
                    && let Some(origin) = origin_document(reading, before)
                {
                    held.retain(|(pointer, _)| {
                        !by_path(pointer) || lookup(&origin, pointer) != lookup(document, pointer)
                    });
                }
                held
            }
            (_, Some(carried), None) => carried
                .pins
                .iter()
                .filter(|(pointer, _)| carried.held.contains(pointer))
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        let followed = carried.or(before);
        Placer {
            reading,
            carried,
            before,
            moves: followed.map(|previous| {
                Moves::at(
                    &previous.text,
                    reading.text,
                    reading.sources.caret,
                    reading.sources.change,
                )
            }),
            pins: carried
                .map(|previous| previous.pins.clone())
                .unwrap_or_default(),
            held,
            hosts_own,
        }
    }

    /// Holds a locked field with no value pinned yet (the host's text never
    /// parsed, or the host named its path after edits were made) that the
    /// text, now parsing, reads otherwise than the host's text (its parse,
    /// or its stand-in's), gone from it included: what was typed meanwhile
    /// is not the host's.
    pub(super) fn hold_unpinned(&mut self) {
        let (Some(previous), Some(document)) =
            (self.reading.sources.previous, &self.reading.document)
        else {
            return;
        };
        let named: Vec<String> = self
            .reading
            .sources
            .paths
            .iter()
            .map(|path| path.to_pointer())
            .collect();
        let pointers = previous
            .locks
            .iter()
            .filter_map(|lock| lock.pointer.clone())
            .chain(named);
        for pointer in pointers {
            if previous.pins.iter().any(|(known, _)| *known == pointer)
                || self.held.iter().any(|(known, _)| *known == pointer)
            {
                continue;
            }
            let host = self
                .origin_value(&pointer)
                .or_else(|| self.read_within_span(&pointer));
            if let Some(host) = host
                && lookup(document, &pointer) != Some(&host)
            {
                self.held.push((pointer, host));
            }
        }
    }

    /// With no host value to compare (the host's text never parsed and has
    /// no stand-in), the value of the field at `pointer` as its lock's span
    /// writes it: when the field's text in this parse runs on past the span
    /// (a line added under a block's end), that text cut off and the rest
    /// read. `None` when it does not run past it, or the rest does not
    /// parse.
    fn read_within_span(&self, pointer: &str) -> Option<Value> {
        let lock = self.moved(pointer)?;
        let end = lock.ranges.iter().map(|range| range.end).max()?;
        let Reading {
            behavior,
            text,
            lines,
            sources,
            ..
        } = self.reading;
        // Where the parse writes the field now.
        let field = self
            .located(pointer)?
            .ranges
            .iter()
            .map(|range| range.end)
            .max()?;
        let past = lines.byte_of(end)..lines.byte_of(field);
        if past.is_empty() || text[past.clone()].trim().is_empty() {
            return None;
        }
        let cut = format!("{}{}", &text[..past.start], &text[past.end..]);
        let document = codec::read(*behavior, &cut, sources.reading_schema).ok()?;
        lookup(&document, pointer).cloned()
    }

    /// Lets go of the fields held for a host range (they were found by
    /// what the range covers) whose value a field the ranges cover now
    /// (`covered`) still reads under the same keys: the range's text moved
    /// with its item to another index (an item added above it, a second
    /// block making a lone one a list), which is no change. Under other
    /// keys (its line moved into another block) it stays held.
    pub(super) fn release_covered(&mut self, covered: &[(String, Reason)]) {
        let (Some(before), Some(document)) = (self.before, &self.reading.document) else {
            return;
        };
        let now: Vec<(Vec<&str>, &Value)> = covered
            .iter()
            .filter_map(|(pointer, _)| Some((keys_of(pointer), lookup(document, pointer)?)))
            .collect();
        let earlier = before.document.as_deref();
        self.held.retain(|(pointer, value)| {
            let range = before.locks.iter().any(|lock| {
                lock.pointer.as_deref() == Some(pointer.as_str()) && lock.reason == Reason::Range
            });
            let keys = keys_of(pointer);
            // Moved, not copied: read under those keys as often as before
            // (a copy of its line makes it a list of two there).
            let segments: Vec<&str> = keys.iter().copied().filter(|key| !key.is_empty()).collect();
            let moved = || {
                earlier.is_some_and(|earlier| {
                    reads_under(earlier, &segments, value)
                        == reads_under(document, &segments, value)
                })
            };
            !(range && now.iter().any(|(at, now)| *at == keys && *now == value) && moved())
        });
    }

    /// The fields a host range covered in the text before, no longer
    /// covered now (the range's text moved away) but still inside one of
    /// `ranges` (the host's, as now placed), that read as pinned
    /// again (a copy of the text put back there): still the host's locked
    /// paths, so a change there holds the save as before (those reading
    /// otherwise are held, see [`Self::held_left`]).
    pub(super) fn covered_before(
        &self,
        covered: &[(String, Reason)],
        ranges: &[Lock],
    ) -> Vec<(String, Reason)> {
        let (Some(before), Some(document)) = (self.before, &self.reading.document) else {
            return Vec::new();
        };
        // Only an edit moves a range's text away, and only while the host
        // gives ranges: new ranges on the same text, or none, cover what
        // they cover, as in a fresh editor.
        let any_range = ranges.iter().any(|range| range.pointer.is_none())
            || !self.reading.sources.range_fields.is_empty();
        if before.text == self.reading.text || !any_range {
            return Vec::new();
        }
        // Its lock, followed here, inside a host range now (not one the
        // host's new ranges no longer cover).
        let in_range = |pointer: &str| {
            self.moved(pointer).is_some_and(|lock| {
                lock.ranges.iter().all(|stretch| {
                    ranges
                        .iter()
                        .filter(|range| range.pointer.is_none())
                        .any(|range| {
                            range
                                .ranges
                                .iter()
                                .any(|host| host.start <= stretch.start && stretch.end <= host.end)
                        })
                })
            })
        };
        // The host's ranges as they were, only followed here (their text is
        // locked, so each keeps its length): a field one covered is still
        // the host's, wherever its lock and the range went among alike
        // lines (a copy of the line: the range may follow one copy, the
        // lock another, and the parse read a third).
        let lengths = |locks: &[Lock]| {
            let mut lengths: Vec<usize> = locks
                .iter()
                .filter(|lock| lock.pointer.is_none())
                .flat_map(|lock| lock.ranges.iter().map(|range| range.len()))
                .collect();
            lengths.sort_unstable();
            lengths
        };
        let same_ranges = lengths(&before.locks) == lengths(ranges);
        before
            .locks
            .iter()
            .filter(|lock| lock.reason == Reason::Range)
            .filter_map(|lock| lock.pointer.as_ref())
            .filter(|pointer| !covered.iter().any(|(known, _)| known == *pointer))
            .filter(|pointer| same_ranges || in_range(pointer))
            .filter(|pointer| {
                before.pins.iter().any(|(known, value)| {
                    known == *pointer && lookup(document, pointer) == Some(value)
                })
            })
            .map(|pointer| (pointer.clone(), Reason::Range))
            .collect()
    }

    /// The fields held that `placed` (the fields placed) leaves out (moved
    /// away, no longer marked where they are now), each with the reason
    /// it was locked for in the text before.
    pub(super) fn held_left(&self, placed: &[Lock]) -> Vec<(String, Reason)> {
        let Some(before) = self.before else {
            return Vec::new();
        };
        self.held
            .iter()
            .filter(|(pointer, _)| {
                !placed
                    .iter()
                    .any(|lock| lock.pointer.as_deref() == Some(pointer))
            })
            .filter_map(|(pointer, _)| {
                let lock = before
                    .locks
                    .iter()
                    .find(|lock| lock.pointer.as_deref() == Some(pointer))?;
                Some((pointer.clone(), lock.reason.clone()))
            })
            .collect()
    }

    /// The fields held whose lock was followed into this text, and those
    /// the text no longer has.
    pub(super) fn held_pointers(&self, locks: &[Lock]) -> Vec<String> {
        // A text that does not parse carries every one held.
        let Some(document) = &self.reading.document else {
            return self
                .held
                .iter()
                .map(|(pointer, _)| pointer.clone())
                .collect();
        };
        let gone: Vec<&String> = self.held_gone(document).collect();
        self.held
            .iter()
            .map(|(pointer, _)| pointer)
            .filter(|pointer| {
                gone.contains(pointer)
                    || locks
                        .iter()
                        .any(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))
            })
            .cloned()
            .collect()
    }

    /// The field at `pointer`, locked for `reason`, placed by the first
    /// step that finds it.
    pub(super) fn place(&mut self, pointer: String, reason: Reason) -> Placed {
        // Followed from the text before by position while the text does
        // not parse, or the field reads otherwise than pinned (held):
        // nothing else says where its pinned text is.
        let moved = self.moved(&pointer);
        let follow =
            self.reading.document.is_none() || self.held.iter().any(|(held, _)| *held == pointer);
        if follow && let Some(lock) = moved.clone() {
            return Placed::Locked(Lock { reason, ..lock });
        }
        if let Some((lock, pin)) = self.stand_in_lock(&pointer) {
            self.pins.push(pin);
            return Placed::Locked(Lock { reason, ..lock });
        }
        // A text that parses places it by its path, as a fresh editor
        // would, where that covers the text followed from the text before
        // (placed by a stand-in or carried while it did not parse), or,
        // from one that parsed, under the same key. A parse of a line half
        // typed may read the user's new text as the field (`password:
        // password: '…'`, the path found at the first of a key written
        // twice, read at the last): the text followed stays the lock. A
        // text that did not parse said nothing sure of where a field is (a
        // stand-in reads around its breaking lines): a lock followed out of
        // it goes where its path says in the host's own text, which has no
        // half-typed line, and where its path reads the value pinned.
        match self.located(&pointer) {
            Some(found) => match moved {
                Some(lock)
                    if !self.hosts_own
                        && !self.repaired_as_pinned(&pointer, &reason)
                        && (!covers(&found.ranges, &lock.ranges)
                            || (self.before_parsed() && lock.key != found.key)) =>
                {
                    Placed::Locked(Lock { reason, ..lock })
                }
                _ => Placed::Locked(Lock {
                    guards: self.ancestor_keys(&pointer),
                    pointer: Some(pointer),
                    key: found.key,
                    ranges: found.ranges,
                    reason,
                }),
            },
            None => match moved {
                // Not found where its path says (a key half typed before
                // it): the text followed.
                Some(lock) => Placed::Locked(Lock { reason, ..lock }),
                // A field a host range covers is found again from the range
                // in each text that parses (the range's own lock keeps the
                // text meanwhile): a half-typed parse's path for it is no
                // field.
                None if reason != Reason::Range && self.kept_unplaced(&pointer) => {
                    Placed::Unplaced(pointer, reason)
                }
                None => Placed::Absent,
            },
        }
    }

    /// The values to pin: in a text that parses, each placed lock's value
    /// there, and each field held unplaced; else those carried and pinned
    /// while placing.
    pub(super) fn into_pins(
        self,
        locks: &[Lock],
        unplaced: &[(String, Reason)],
    ) -> Vec<(String, Value)> {
        // Each field once (one held may be gone from the parse and keep its
        // lock, followed by position).
        let mut seen = std::collections::HashSet::new();
        match &self.reading.document {
            None => self.pins,
            Some(document) => locks
                .iter()
                .filter_map(|lock| lock.pointer.as_ref())
                .chain(unplaced.iter().map(|(pointer, _)| pointer))
                .chain(self.held_gone(document))
                .filter(|pointer| seen.insert(pointer.as_str()))
                .filter_map(|pointer| {
                    // A field held keeps the host's value (see
                    // `hold_unpinned` for one named late).
                    if let Some((_, value)) = self.held.iter().find(|(known, _)| known == pointer) {
                        return Some((pointer.clone(), value.clone()));
                    }
                    Some((pointer.clone(), lookup(document, pointer).cloned()?))
                })
                .collect(),
        }
    }

    /// The fields held that `document` no longer has (a locked line
    /// deleted or commented out): held until it reads them again.
    fn held_gone<'s>(&'s self, document: &'s Value) -> impl Iterator<Item = &'s String> {
        self.held
            .iter()
            .map(|(pointer, _)| pointer)
            .filter(move |pointer| lookup(document, pointer).is_none())
    }

    /// Whether the text before parsed (its locks were placed by path).
    fn before_parsed(&self) -> bool {
        self.before.is_some_and(|before| before.document.is_some())
    }

    /// Whether this text, which parses after a text that did not, reads
    /// the field at `pointer` (named by its path, for `reason`) as the text
    /// before pinned it. A field a host range covers is no path: its list
    /// index moves with items added, so its pin, read at its pointer, says
    /// nothing of where it is.
    fn repaired_as_pinned(&self, pointer: &str, reason: &Reason) -> bool {
        let (Some(before), Some(document)) = (self.before, &self.reading.document) else {
            return false;
        };
        // The document before is kept only when something needed it: the
        // parse says whether that text parsed.
        *reason != Reason::Range
            && before
                .pins
                .iter()
                .find(|(known, _)| known == pointer)
                .is_some_and(|(_, pinned)| lookup(document, pointer) == Some(pinned))
            && before.document.is_none()
            && parse_cache::parsed(self.reading.behavior, &before.text).is_err()
    }

    /// The lock of the text before, moved through the edit, when the edit
    /// kept its text.
    fn moved(&self, pointer: &str) -> Option<Lock> {
        let (previous, moves) = self.carried.or(self.before).zip(self.moves.as_ref())?;
        let lock = previous
            .locks
            .iter()
            .find(|lock| lock.pointer.as_deref() == Some(pointer))?;
        moves.lock(lock)
    }

    /// In a text that does not parse, the stand-in's lock with its value
    /// to pin, unless a value was pinned in a text before.
    fn stand_in_lock(&self, pointer: &str) -> Option<(Lock, (String, Value))> {
        let pinned = self.pins.iter().any(|(known, _)| *known == pointer);
        if self.reading.document.is_some() || pinned {
            return None;
        }
        let stand = &self.reading.stand_in()?.protected;
        let lock = stand
            .locks
            .iter()
            .find(|lock| lock.pointer.as_deref() == Some(pointer))?;
        let pin = stand.pins.iter().find(|(known, _)| *known == pointer)?;
        // A lock named after edits were made: its value is the host's.
        let pin = match self.origin_value(pointer) {
            Some(value) => (pointer.to_string(), value),
            None => pin.clone(),
        };
        Some((lock.clone(), pin))
    }

    /// The value of the field at `pointer` in the text from the host the
    /// locks follow, when that is another text than this one: its parse,
    /// or its stand-in's.
    fn origin_value(&self, pointer: &str) -> Option<Value> {
        let origin = self.reading.sources.previous?.origin.as_deref()?;
        if origin == self.reading.text {
            return None;
        }
        let Reading {
            behavior, sources, ..
        } = self.reading;
        let reading = sources.reading_schema;
        let document = codec::read(*behavior, origin, reading).ok().or_else(|| {
            let stand = scan::stand_in(*behavior, origin, None, reading)?;
            codec::read(*behavior, &stand.text, reading).ok()
        })?;
        lookup(&document, pointer).cloned()
    }

    /// Located in the text, with what is inside it as the parsed text (or
    /// the value pinned in the last one that parsed) has it; without a
    /// value, the lines written inside it. Not found where its path says (a
    /// bracket left open above it moves every line below into the list):
    /// the lines written under its own key wherever they seem to be, so an
    /// unreadable text locks more rather than less.
    fn located(&self, pointer: &str) -> Option<Extent> {
        let Reading {
            behavior,
            text,
            lines,
            document,
            ..
        } = self.reading;
        let value = self.value(pointer);
        let inside = match (value, document) {
            (None, None) => Some(self.reading.written()),
            _ => None,
        };
        extent(*behavior, text, lines, value, inside, pointer).or_else(|| {
            inside.and_then(|written| scan::under_own_key(*behavior, text, lines, written, pointer))
        })
    }

    /// The value of the field at `pointer`: in the parsed text, else the
    /// one pinned in the last text that parsed.
    fn value(&self, pointer: &str) -> Option<&Value> {
        match &self.reading.document {
            Some(document) => lookup(document, pointer),
            None => self
                .pins
                .iter()
                .find(|(pinned, _)| *pinned == pointer)
                .map(|(_, value)| value),
        }
    }

    /// The fields whose spans placing `pointers` looks up: each one and
    /// those inside it (see `locate::extent`), to read in one batch.
    pub(super) fn spans_wanted(&self, pointers: &[(String, Reason)]) -> Vec<String> {
        let mut wanted = Vec::with_capacity(pointers.len());
        for (pointer, _) in pointers {
            wanted.push(pointer.clone());
            if let Some(value) = self.value(pointer) {
                wanted.extend(inside(value, pointer));
            }
        }
        wanted
    }

    /// Whether a field not found is kept unplaced: in a text that parses,
    /// when the document has it (the language could not say where it is
    /// written: its value is pinned, and an edit changing it refused); in a
    /// text that does not parse, until it does, when it was found or kept
    /// so in the text before, or written in the last text that parsed or
    /// the stand-in. Else it is not written (a path the text never had):
    /// nothing to keep until the text parses and the field is placed.
    fn kept_unplaced(&self, pointer: &str) -> bool {
        if let Some(document) = &self.reading.document {
            return lookup(document, pointer).is_some();
        }
        let has = |document: &Value| lookup(document, pointer).is_some();
        let last = self.carried.and_then(|previous| previous.parsed.as_deref());
        let stand = self
            .reading
            .stand_in()
            .and_then(|stand| stand.protected.document.as_deref());
        let before = self.carried.is_some_and(|previous| {
            previous.unplaced.iter().any(|(known, _)| *known == pointer)
                || previous
                    .locks
                    .iter()
                    .any(|lock| lock.pointer.as_deref() == Some(pointer))
        });
        before || last.is_some_and(has) || stand.is_some_and(has)
    }

    /// The keys of the fields holding `pointer`.
    fn ancestor_keys(&self, pointer: &str) -> Vec<Range<usize>> {
        let Reading {
            behavior,
            text,
            lines,
            ..
        } = self.reading;
        let mut keys = Vec::new();
        let mut at = pointer;
        while let Some(cut) = at.rfind('/') {
            at = &at[..cut];
            if at.is_empty() {
                break;
            }
            if let Some(range) = key_range(*behavior, text, at) {
                keys.push(lines.range(range));
            }
        }
        keys
    }
}

/// The parse of the host's text the locks of `before` follow, when it is
/// another text than `reading`'s and parses.
fn origin_document(reading: &Reading<'_>, before: &Protected) -> Option<Value> {
    let origin = before.origin.as_deref()?;
    if origin == reading.text {
        return None;
    }
    codec::read(reading.behavior, origin, reading.sources.reading_schema).ok()
}

/// The pinned values of the text before (`before`, whose locks are its
/// own) that read otherwise in `document`: the fields to hold.
fn held_pins(before: &Protected, document: &Value) -> Vec<(String, Value)> {
    before
        .pins
        .iter()
        .filter(|(pointer, value)| {
            lookup(document, pointer) != Some(value)
                && (before.held.contains(pointer)
                    || before
                        .locks
                        .iter()
                        .any(|lock| lock.pointer.as_deref() == Some(pointer.as_str())))
        })
        .cloned()
        .collect()
}

/// The list items a host range covers that `document` reads fewer times
/// under their list than the text before did (`before`, which parsed),
/// each with the value to hold it at: a list item's pointer moves with the
/// items around it, so its pin, read at the pointer, may be another item's
/// while its own line went where no list reads it (moved under a scalar,
/// its line deleted below a copy). Items are matched by their span's text,
/// which holds the value the host gave.
fn items_read_fewer(before: &Protected, document: &Value) -> Vec<(String, Value)> {
    let Some(earlier) = before.document.as_deref() else {
        return Vec::new();
    };
    let units: Vec<u16> = before.text.encode_utf16().collect();
    let mut found: Vec<(String, Value)> = Vec::new();
    for lock in &before.locks {
        let Some(pointer) = lock
            .pointer
            .as_deref()
            .filter(|_| lock.reason == Reason::Range)
        else {
            continue;
        };
        let keys: Vec<&str> = keys_of(pointer)
            .into_iter()
            .filter(|key| !key.is_empty())
            .collect();
        if keys.len() == pointer.split('/').filter(|key| !key.is_empty()).count() {
            continue;
        }
        let written: String = lock
            .ranges
            .iter()
            .map(|range| String::from_utf16_lossy(units.get(range.clone()).unwrap_or_default()))
            .collect();
        let mut values = Vec::new();
        values_under(earlier, &keys, &mut values);
        let dropped = values.into_iter().find(|value| {
            let text = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            !text.is_empty()
                && written.contains(&text)
                && reads_under(document, &keys, value) < reads_under(earlier, &keys, value)
        });
        if let Some(value) = dropped
            && !found.iter().any(|(_, held)| held == value)
        {
            found.push((pointer.to_string(), value.clone()));
        }
    }
    found
}

/// Whether each of `inner` lies inside one of `outer`.
fn covers(outer: &[Range<usize>], inner: &[Range<usize>]) -> bool {
    inner.iter().all(|inner| {
        outer
            .iter()
            .any(|outer| outer.start <= inner.start && inner.end <= outer.end)
    })
}

/// How many fields under `keys` (any list index between them) of `value`
/// (a document) read `expected`.
fn reads_under(value: &Value, keys: &[&str], expected: &Value) -> usize {
    match value {
        _ if keys.is_empty() && value == expected => 1,
        Value::Array(items) => items
            .iter()
            .map(|item| reads_under(item, keys, expected))
            .sum(),
        _ if keys.is_empty() => 0,
        Value::Object(map) => map
            .get(&keys[0].replace("~1", "/").replace("~0", "~"))
            .map_or(0, |child| reads_under(child, &keys[1..], expected)),
        _ => 0,
    }
}

/// The keys of `pointer`, its list indices left out.
fn keys_of(pointer: &str) -> Vec<&str> {
    pointer
        .split('/')
        .filter(|segment| segment.parse::<usize>().is_err())
        .collect()
}
