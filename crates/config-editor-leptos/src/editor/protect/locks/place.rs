//! Where each locked field is in the text, tried in order: in a text that
//! does not parse, the lock of the text before moved through the edit,
//! else the stand-in's lock (its value pinned); else located by its path
//! (or under its own key); else held as a field not found: in a text that
//! does not parse, until it parses; in one that parses (a language that
//! cannot locate fields), by its value alone, every edit then compared with
//! it (`guard`).

use std::ops::Range;

use serde_json::Value;

use super::super::super::paths::key_range;
use super::super::carry::Moves;
use super::super::locate::{Extent, extent, lookup};
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
    moves: Option<Moves<'a>>,
    /// The values pinned in the texts before, then those of each stand-in
    /// lock placed.
    pins: Vec<(String, Value)>,
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
    pub(super) fn new(reading: &'r Reading<'a>, carried: Option<&'a Protected>) -> Self {
        Placer {
            reading,
            carried,
            moves: carried.map(|previous| Moves::new(&previous.text, reading.text)),
            pins: carried
                .map(|previous| previous.pins.clone())
                .unwrap_or_default(),
        }
    }

    /// The field at `pointer`, locked for `reason`, placed by the first
    /// step that finds it.
    pub(super) fn place(&mut self, pointer: String, reason: Reason) -> Placed {
        if let Some(lock) = self.moved(&pointer) {
            return Placed::Locked(Lock { reason, ..lock });
        }
        if let Some((lock, pin)) = self.stand_in_lock(&pointer) {
            self.pins.push(pin);
            return Placed::Locked(Lock { reason, ..lock });
        }
        match self.located(&pointer) {
            Some(found) => Placed::Locked(Lock {
                guards: self.ancestor_keys(&pointer),
                pointer: Some(pointer),
                key: found.key,
                ranges: found.ranges,
                reason,
            }),
            None if self.held(&pointer) => Placed::Unplaced(pointer, reason),
            None => Placed::Absent,
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
        match &self.reading.document {
            None => self.pins,
            Some(document) => locks
                .iter()
                .filter_map(|lock| lock.pointer.as_ref())
                .chain(unplaced.iter().map(|(pointer, _)| pointer))
                .filter_map(|pointer| Some((pointer.clone(), lookup(document, pointer)?.clone())))
                .collect(),
        }
    }

    /// In a text that does not parse, the lock of the text before, moved
    /// through the edit, when the edit kept its text.
    fn moved(&self, pointer: &str) -> Option<Lock> {
        let (previous, moves) = self.carried.zip(self.moves.as_ref())?;
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
        Some((lock.clone(), pin.clone()))
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
        let value = match document {
            Some(document) => lookup(document, pointer),
            None => self
                .pins
                .iter()
                .find(|(pinned, _)| *pinned == pointer)
                .map(|(_, value)| value),
        };
        let inside = match (value, document) {
            (None, None) => Some(self.reading.written()),
            _ => None,
        };
        extent(*behavior, text, lines, value, inside, pointer).or_else(|| {
            inside.and_then(|written| scan::under_own_key(*behavior, text, lines, written, pointer))
        })
    }

    /// Whether a field not found is held: in a text that parses, when the
    /// document has it (the language could not say where it is written:
    /// its value is pinned, and an edit changing it refused); in a text
    /// that does not parse, until it does, when it was found or held so in
    /// the text before, or written in the last text that parsed or the
    /// stand-in. Else it is not written (a path the text never had):
    /// nothing to keep until the text parses, where `guard::repair` reads
    /// it again.
    fn held(&self, pointer: &str) -> bool {
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
