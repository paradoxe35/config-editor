//! Where each locked field is in the text.
//!
//! - A text that parses places it where its parse writes it, by its path
//!   (`locate::extent`), as a fresh editor does. Only when that path names
//!   other text than the field's lock followed from a text before that
//!   parsed (a list item's index another item's now, a key typed again
//!   ahead of it) is the field held on the text it holds; when the path
//!   names nothing, the lock followed stays.
//! - A text that does not parse follows the locks of the text before
//!   through the edit; a field with none to follow takes the stand-in's
//!   lock (its value pinned), else is read off the lines (`scan`), else is
//!   kept unplaced until the text parses.
//! - A field the language cannot locate (no spans) is kept unplaced, by
//!   its value: a text reading it otherwise holds it.

use super::super::super::text::{TextRange, TextSize, slice};

use serde_json::Value;

use super::super::super::codec;
use super::super::super::paths::{inside, key_range};
use super::super::carry::Moves;
use super::super::locate::{Extent, extent, lookup, values_under};
use super::super::scan;
use super::super::texts::Reason;
use super::holds::{EditMade, HoldCause, Holds};
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
    /// The fields held, with their pinned values and why (see
    /// `Protected::held`).
    held: Vec<Pin>,
    /// Whether the text is the host's own (the one the locks follow): it
    /// writes every field where the host did, so its paths place them.
    hosts_own: bool,
    /// The fields placed so far whose locks are a reading of a text that
    /// does not parse (see `Protected::read_off`).
    read_off: Vec<String>,
}

/// A field held: the value pinned for it, and why it is held.
struct Pin {
    pointer: String,
    value: Value,
    cause: HoldCause,
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
        // Held through a text that does not parse, as pinned. Each keeps
        // the cause it was held for; one held by this edit has its cause
        // read off the edit below (`None` until then).
        let held: Vec<(String, Value, Option<HoldCause>)> =
            match (before, carried, &reading.document) {
                _ if hosts_own => Vec::new(),
                (Some(before), _, Some(document)) => {
                    let mut held: Vec<(String, Value, Option<HoldCause>)> =
                        held_pins(before, document)
                            .into_iter()
                            .map(|(pointer, value)| {
                                let cause = before.held.cause(&pointer);
                                (pointer, value, cause)
                            })
                            .collect();
                    for (pointer, value) in items_read_fewer(before, document) {
                        if !held.iter().any(|(known, ..)| *known == pointer) {
                            let cause =
                                before.held.cause(&pointer).unwrap_or(HoldCause::RangeMoved);
                            held.push((pointer, value, Some(cause)));
                        }
                    }
                    one_hold_per_range_field(before, document, &mut held);
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
                    if held.iter().any(|(pointer, ..)| by_path(pointer))
                        && let Some(origin) = origin_document(reading, before)
                    {
                        held.retain(|(pointer, ..)| {
                            !by_path(pointer)
                                || lookup(&origin, pointer) != lookup(document, pointer)
                        });
                    }
                    held
                }
                (_, Some(carried), None) => carried
                    .pins
                    .iter()
                    .filter_map(|(pointer, value)| {
                        let cause = carried.held.cause(pointer)?;
                        Some((pointer.clone(), value.clone(), Some(cause)))
                    })
                    .collect(),
                _ => Vec::new(),
            };
        let followed = carried.or(before);
        let mut placer = Placer {
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
            held: Vec::new(),
            hosts_own,
            read_off: Vec::new(),
        };
        placer.held = placer.with_causes(held);
        placer
    }

    /// `held` with the cause of each field this edit holds read off the
    /// edit (see `holds::EditMade`), the others' kept.
    fn with_causes(&self, held: Vec<(String, Value, Option<HoldCause>)>) -> Vec<Pin> {
        let edit = self
            .before
            .zip(self.moves.as_ref())
            .filter(|_| held.iter().any(|(_, _, cause)| cause.is_none()));
        let Some((before, moves)) = edit else {
            return held
                .into_iter()
                .map(|(pointer, value, cause)| Pin {
                    pointer,
                    value,
                    cause: cause.unwrap_or(HoldCause::Unclear),
                })
                .collect();
        };
        let Reading {
            behavior,
            text,
            lines,
            document,
            ..
        } = self.reading;
        let comments = behavior.comments(text);
        let in_comment = |offset: TextSize| {
            let byte = usize::from(offset);
            comments
                .iter()
                .any(|range| range.start <= byte && byte < range.end)
        };
        let pieces = moves.pieces();
        let made = EditMade {
            before,
            behavior: *behavior,
            after: text,
            after_lines: lines,
            pieces: &pieces,
            in_comment: &in_comment,
        };
        held.into_iter()
            .map(|(pointer, value, cause)| {
                let cause = cause.unwrap_or_else(|| {
                    let gone = document
                        .as_ref()
                        .is_none_or(|document| lookup(document, &pointer).is_none());
                    // Where the parse writes it now, looked up only when the
                    // edit's place says no more.
                    let now = || {
                        let ranges = self.located(&pointer)?.ranges;
                        Some(TextRange::new(
                            ranges.first()?.start(),
                            ranges.last()?.end(),
                        ))
                    };
                    made.cause(&pointer, self.moved(&pointer).as_ref(), gone, &now)
                });
                Pin {
                    pointer,
                    value,
                    cause,
                }
            })
            .collect()
    }

    /// Whether the field at `pointer` is held (see `Protected::held`).
    pub(super) fn holds(&self, pointer: &str) -> bool {
        self.held.iter().any(|pin| pin.pointer == pointer)
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
                || self.held.iter().any(|pin| pin.pointer == pointer)
            {
                continue;
            }
            let host = self
                .origin_value(&pointer)
                .or_else(|| self.read_within_span(&pointer));
            if let Some(host) = host
                && lookup(document, &pointer) != Some(&host)
            {
                let cause = if lookup(document, &pointer).is_none() {
                    HoldCause::Removed
                } else {
                    HoldCause::ChangedBeforeLock
                };
                self.held.push(Pin {
                    pointer,
                    value: host,
                    cause,
                });
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
        let end = lock.ranges.iter().map(|range| range.end()).max()?;
        let Reading {
            behavior,
            text,
            sources,
            ..
        } = self.reading;
        // Where the parse writes the field now.
        let field = self
            .located(pointer)?
            .ranges
            .iter()
            .map(|range| range.end())
            .max()?;
        if field <= end {
            return None;
        }
        let past = TextRange::new(end, field);
        if slice::slice(text, past)?.trim().is_empty() {
            return None;
        }
        let cut = format!(
            "{}{}",
            slice::before(text, past.start())?,
            slice::after(text, past.end())?
        );
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
        self.held.retain(|Pin { pointer, value, .. }| {
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
        // they cover, as in a fresh editor. The same ranges on the same
        // text (the locks read again with nothing changed) keep what the
        // text before kept: reading again is no edit.
        let any_range = ranges.iter().any(|range| range.pointer.is_none())
            || !self.reading.sources.range_fields.is_empty();
        let new_ranges = before.ranges != super::range_stretches(ranges)
            || before.range_fields != self.reading.sources.range_fields;
        if (*before.text == *self.reading.text && new_ranges) || !any_range {
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
                                .any(|host| host.contains_range(*stretch))
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
            let mut lengths: Vec<TextSize> = locks
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
            .map(|pin| &pin.pointer)
            .filter(|pointer| {
                !placed
                    .iter()
                    .any(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))
            })
            .filter_map(|pointer| {
                let lock = before
                    .locks
                    .iter()
                    .find(|lock| lock.pointer.as_deref() == Some(pointer))?;
                Some((pointer.clone(), lock.reason.clone()))
            })
            .collect()
    }

    /// The fields held whose lock was followed into this text, those kept
    /// unplaced (`unplaced`: no span says where they are), and those the
    /// text no longer has.
    pub(super) fn held_pointers(&self, locks: &[Lock], unplaced: &[(String, Reason)]) -> Holds {
        let entry = |pin: &Pin| (pin.pointer.clone(), pin.cause);
        // A text that does not parse carries every one held.
        let Some(document) = &self.reading.document else {
            return Holds::new(self.held.iter().map(entry).collect());
        };
        let gone: Vec<&String> = self.held_gone(document).collect();
        let kept = self.held.iter().filter(|pin| {
            let pointer = &pin.pointer;
            gone.contains(&pointer)
                || unplaced.iter().any(|(known, _)| known == pointer)
                || locks
                    .iter()
                    .any(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))
        });
        Holds::new(kept.map(entry).collect())
    }

    /// The field at `pointer`, locked for `reason`, placed by the first
    /// step that finds it.
    pub(super) fn place(&mut self, pointer: String, reason: Reason) -> Placed {
        // Changed before the host named it, its text on screen is the
        // user's, not the host's: like a host range whose text was changed
        // before it was given, it locks no span (every way back is taken)
        // and is held at the host's value, placed by its path once it
        // reads so again.
        if self
            .held
            .iter()
            .any(|pin| pin.pointer == pointer && pin.cause == HoldCause::ChangedBeforeLock)
        {
            return Placed::Unplaced(pointer, reason);
        }
        // Followed from the text before by position while the text does
        // not parse, or the field reads otherwise than pinned (held):
        // nothing else says where its pinned text is.
        let moved = self.moved(&pointer);
        let follow =
            self.reading.document.is_none() || self.held.iter().any(|pin| pin.pointer == pointer);
        if follow && let Some(lock) = moved.clone() {
            if self.was_read_off(&pointer) {
                self.read_off.push(pointer.clone());
            }
            return Placed::Locked(Lock { reason, ..lock });
        }
        if let Some((lock, pin)) = self.stand_in_lock(&pointer) {
            self.pins.push(pin);
            self.read_off.push(pointer.clone());
            return Placed::Locked(Lock { reason, ..lock });
        }
        // A text that parses places it by its path, as a fresh editor
        // would: the parse knows where its value is written. Only a text
        // that does not parse follows the text before (above).
        match self.located(&pointer) {
            // Its path names other text than the field's (a list item's
            // index another item's now; a key typed again ahead of it,
            // which the language's spans find first while its parse reads
            // the last): the field is held, on the text it holds, until the
            // text writes it where its path says again.
            Some(found)
                if let Some(lock) = moved
                    .as_ref()
                    .filter(|lock| self.names_other_text(lock, &found))
                    .cloned() =>
            {
                self.hold(&pointer);
                Placed::Locked(Lock { reason, ..lock })
            }
            Some(found) => {
                if self.reading.document.is_none() {
                    self.read_off.push(pointer.clone());
                }
                Placed::Locked(Lock {
                    guards: self.ancestor_keys(&pointer),
                    pointer: Some(pointer),
                    key: found.key,
                    ranges: found.ranges,
                    reason,
                })
            }
            None => match moved {
                // Not found where its path says (a text that does not
                // parse, a key half typed before it that the language's
                // spans read as another): the text followed.
                Some(lock) => {
                    if self.was_read_off(&pointer) {
                        self.read_off.push(pointer.clone());
                    }
                    Placed::Locked(Lock { reason, ..lock })
                }
                // A field a host range covers is found again from the range
                // in each text that parses (the range's own lock keeps the
                // text meanwhile): a half-typed parse's path for it is no
                // field.
                _ if reason != Reason::Range && self.kept_unplaced(&pointer) => {
                    Placed::Unplaced(pointer, reason)
                }
                _ => Placed::Absent,
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
                    if let Some(Pin { value, .. }) =
                        self.held.iter().find(|pin| pin.pointer == *pointer)
                    {
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
            .map(|pin| &pin.pointer)
            .filter(move |pointer| lookup(document, pointer).is_none())
    }

    /// Whether the lock `found` by its path covers other text than
    /// `followed`, the field's lock followed from the text before, placed
    /// by a parse: whitespace aside, their values are written differently
    /// (a lock's text never changes, so the path names another value).
    fn names_other_text(&self, followed: &Lock, found: &Extent) -> bool {
        let pointer = followed.pointer.as_deref().unwrap_or_default();
        !self.hosts_own
            && !self.was_read_off(pointer)
            && self.written(&followed.ranges) != self.written(&found.ranges)
    }

    /// Whether the lock of the text before for `pointer` was read off a
    /// text that does not parse.
    fn was_read_off(&self, pointer: &str) -> bool {
        self.carried
            .or(self.before)
            .is_some_and(|previous| previous.read_off.iter().any(|known| known == pointer))
    }

    /// The fields among `locks` whose locks are a reading of a text that
    /// does not parse (see `Protected::read_off`).
    pub(super) fn read_off(&self, locks: &[Lock]) -> Vec<String> {
        self.read_off
            .iter()
            .filter(|pointer| {
                locks
                    .iter()
                    .any(|lock| lock.pointer.as_deref() == Some(pointer.as_str()))
            })
            .cloned()
            .collect()
    }

    /// The text of `ranges`, whitespace left out.
    fn written(&self, ranges: &[TextRange]) -> String {
        let text = self.reading.text;
        ranges
            .iter()
            .flat_map(|range| slice::slice(text, *range).unwrap_or_default().chars())
            .filter(|character| !character.is_whitespace())
            .collect()
    }

    /// Holds the field at `pointer` at the value pinned in the text before,
    /// its path naming other text than its own: a list item's index naming
    /// another item, else its key written again ahead of it.
    fn hold(&mut self, pointer: &str) {
        let item = pointer
            .rsplit('/')
            .next()
            .is_some_and(|last| last.parse::<usize>().is_ok());
        let cause = if item {
            HoldCause::ListItem
        } else {
            HoldCause::Duplicate
        };
        let pin = self.before.and_then(|before| {
            before
                .pins
                .iter()
                .find(|(known, _)| known == pointer)
                .map(|(_, value)| value.clone())
        });
        if let Some(value) = pin
            && !self.held.iter().any(|pin| pin.pointer == pointer)
        {
            self.held.push(Pin {
                pointer: pointer.to_string(),
                value,
                cause,
            });
        }
    }

    /// The lock of the text before, moved through the edit, when the edit
    /// kept its text.
    fn moved(&self, pointer: &str) -> Option<Lock> {
        let (previous, moves) = self.carried.or(self.before).zip(self.moves.as_ref())?;
        let lock = previous
            .locks
            .iter()
            .find(|lock| lock.pointer.as_deref() == Some(pointer))?;
        let moved = moves.lock(lock)?;
        if moved.guards.len() == lock.guards.len() {
            return Some(moved);
        }
        Some(self.held_again(pointer, &previous.text, lock, moved))
    }

    /// `moved`, the lock of the field at `pointer` (`was` in the text
    /// `before`) moved through an edit that took away a key holding it:
    /// in a text that parses, held by the keys the parse writes its text
    /// under when they read as the keys it lost (the key of a list copied
    /// above its items, the first one cut: the copy holds the items).
    /// Else as moved, a holder lost.
    fn held_again(&self, pointer: &str, before: &str, was: &Lock, moved: Lock) -> Lock {
        let Reading {
            behavior,
            text,
            lines,
            document,
            ..
        } = self.reading;
        let span = moved
            .ranges
            .first()
            .map(|first| first.start())
            .zip(moved.ranges.last().map(|last| last.end()));
        let (Some(_), Some((start, end))) = (document, span) else {
            return moved;
        };
        let mut keys = Vec::new();
        for ((key, at), lost) in self.holders(pointer).zip(&was.guards) {
            let holds = extent(*behavior, text, lines, at).is_some_and(|field| {
                field
                    .ranges
                    .first()
                    .is_some_and(|first| first.start() <= start)
                    && field.ranges.last().is_some_and(|last| {
                        // A language whose spans find no value under the
                        // key (only the key itself) says nothing of where
                        // its value ends: the range after the key is held.
                        end <= last.end() || last.end() <= key.end()
                    })
            });
            if !holds || slice::slice(before, *lost) != slice::slice(text, key) {
                return moved;
            }
            keys.push(key);
        }
        if keys.len() != was.guards.len() {
            return moved;
        }
        Lock {
            guards: keys,
            ..moved
        }
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

    /// Where the field at `pointer` is written: in a text that parses,
    /// where the parse writes it. In a text that does not parse (a field
    /// whose lock could not be followed into it, a path named while it
    /// does not parse, a host text that never parsed), read off its lines
    /// (`scan::read_off`): the fields inside it as the value pinned in the
    /// last text that parsed has them, else the lines written inside it;
    /// not found where its path says (a bracket left open above it moves
    /// every line below into the list), the lines written under its own key
    /// wherever they seem to be, so such a text locks more rather than less.
    fn located(&self, pointer: &str) -> Option<Extent> {
        let Reading {
            behavior,
            text,
            lines,
            document,
            ..
        } = self.reading;
        if document.is_some() {
            return extent(*behavior, text, lines, pointer);
        }
        match self.value(pointer) {
            Some(value) => scan::read_off(*behavior, text, lines, Some(value), &[], pointer),
            None => {
                let written = self.reading.written();
                scan::read_off(*behavior, text, lines, None, written, pointer)
                    .or_else(|| scan::under_own_key(*behavior, text, lines, written, pointer))
            }
        }
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
    fn ancestor_keys(&self, pointer: &str) -> Vec<TextRange> {
        self.holders(pointer).map(|(key, _)| key).collect()
    }

    /// The fields holding `pointer`, innermost first: each one's key and
    /// path.
    fn holders<'p>(&self, pointer: &'p str) -> impl Iterator<Item = (TextRange, &'p str)> {
        let Reading {
            behavior,
            text,
            lines,
            ..
        } = self.reading;
        let mut at = pointer;
        std::iter::from_fn(move || {
            loop {
                at = at.get(..at.rfind('/')?).unwrap_or_default();
                if at.is_empty() {
                    return None;
                }
                if let Some(range) = key_range(*behavior, text, at) {
                    return Some((lines.range(range), at));
                }
            }
        })
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

/// Keeps one hold per field a host range covers. A key holding it renamed
/// a keystroke at a time (`web`, `weba`, `webap`) has the range's text read
/// under each name in between, pinned there: those names are the same
/// locked field, held already under the name it was first held under (the
/// host's). A field held by this edit (`None` cause) that `document` no
/// longer has, whose lock in `before` stands on the same text as the lock
/// of a field held before, is not held again.
fn one_hold_per_range_field(
    before: &Protected,
    document: &Value,
    held: &mut Vec<(String, Value, Option<HoldCause>)>,
) {
    let range_lock = |pointer: &str| {
        before
            .locks
            .iter()
            .find(|lock| lock.pointer.as_deref() == Some(pointer) && lock.reason == Reason::Range)
    };
    let held_on: Vec<&[TextRange]> = held
        .iter()
        .filter(|(_, _, cause)| cause.is_some())
        .filter_map(|(pointer, ..)| range_lock(pointer))
        .map(|lock| lock.ranges.as_slice())
        .collect();
    if held_on.is_empty() {
        return;
    }
    held.retain(|(pointer, _, cause)| {
        cause.is_some()
            || lookup(document, pointer).is_some()
            || !range_lock(pointer).is_some_and(|lock| held_on.contains(&lock.ranges.as_slice()))
    });
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
                    || before.unplaced.iter().any(|(known, _)| known == pointer)
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
            .map(|range| slice::slice(&before.text, *range).unwrap_or_default())
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
            .get(&crate::editor::language::unescaped_key(keys[0]))
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
