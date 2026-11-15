//! The host's read-only ranges over time: one model for a `ConfigEditor`
//! and each document of a `ConfigDocuments`.
//!
//! The ranges a host gives belong to the text it gave them for. A document
//! keeps them as **given**: read as positions in that text. What its editor
//! is handed (the `read_only_ranges` it places and then follows through the
//! user's edits) is derived from them by following that text to the
//! document's:
//!
//! - ranges given with a text that applies are handed as read;
//! - given with an echo or an older text (typed past, `rangesFor`), they
//!   are followed to the document as an edit (`remap::moved_between`: a
//!   range around a change grows over it);
//! - a text of the host's own that replaces the document without ranges (a
//!   reset, a new `value` alone) hands them carried there by line
//!   (`remap::carried`), afresh from the given ones each time, so resets in
//!   one task compose (QA 38, H38-1, H38-2, H38-3);
//! - the same ranges given again (the same JSON, no new host text: a
//!   framework's render, a list given again) stay where the editor moved
//!   them; after a text of the host's own they are a statement about that
//!   text, read in it (QA 38, FW38-1);
//! - ranges that name places the text they come with lacks, but fit the
//!   text the last ones came with, were worked out on that one (a render
//!   listing a newer text with ranges from the one it rendered before: QA
//!   38, FW38-3) and are read there (`protect::ranges_in`);
//! - ranges given with a text from before a reset the host has not given
//!   back yet (`Echoes::before_reset`) are for a text it replaced: they are
//!   not taken, and the carried ones stay.
//!
//! Every rule decides from this one record per document; nothing else
//! keeps the host's ranges.

use std::collections::HashMap;
use std::rc::Rc;

use config_editor_leptos::TextRange;
use wasm_bindgen::JsValue;

use crate::remap::carried;

/// What a list of ranges is compared by: its JSON. `None` when it has none
/// (a cycle, a value JSON cannot write): it then equals no other.
pub(crate) fn print(list: &JsValue) -> Option<String> {
    js_sys::JSON::stringify(list)
        .ok()
        .and_then(|text| text.as_string())
}

/// Whether ranges given again (`again`: equal to the last ones given, no
/// `rangesFor`) stay where the editor moved them: unless the text they
/// come with replaces the document (`replaces`). A text the document
/// holds, an echo of one it sent or one older than the typing keeps them,
/// whatever order the echoes come back in. Whether it replaces is known
/// before the text is taken (`Echoes::replaces`), so ranges that stay are
/// not read in it at all: not checked against a text they were not given
/// for (QA 34, HO34-1: an echo warned of columns past a line's end).
pub(crate) fn stay(again: bool, replaces: bool) -> bool {
    again && !replaces
}

/// What became of a document's ranges listed again as given last.
pub(crate) enum Again {
    /// Its text does not replace the one its editor holds (that text, an
    /// echo): they are the ones its editor was given, which it moved with
    /// each edit since. Not in the listed text's places, they are not moved
    /// from it to the document (QA 34: moved as if they were, a lock whose
    /// line ended past the end of the line that took its place above it
    /// was given again on that line).
    Stayed,
    /// Its text replaces the one its editor holds: they were read in it.
    /// These are the ones its editor was given, which stay if that text
    /// turns out not to replace its own after all.
    Read(Vec<TextRange>),
}

/// One document's host ranges: as given, in the text they name places in.
#[derive(Clone, Default)]
pub(crate) struct HostLocks {
    /// Their JSON, while ranges given again are compared with them: `None`
    /// once a text of the host's own replaced the document.
    print: Option<String>,
    /// The text they came with (where a host worked out the next ones,
    /// when those do not fit theirs).
    came_with: Rc<str>,
    /// The text their positions are in, and the positions: what the
    /// editor was handed from them.
    text: Rc<str>,
    ranges: Vec<TextRange>,
}

impl HostLocks {
    /// `ranges`, given as `print` with `came_with`, read as positions in
    /// `text` (its texts kept only while there are ranges).
    pub(crate) fn given(
        print: Option<String>,
        came_with: &str,
        text: &str,
        ranges: Vec<TextRange>,
    ) -> Self {
        let kept = |text: &str| -> Rc<str> {
            if ranges.is_empty() {
                Rc::from("")
            } else {
                Rc::from(text)
            }
        };
        let came_with = kept(came_with);
        let text = if *text == *came_with {
            came_with.clone()
        } else {
            kept(text)
        };
        Self {
            print,
            came_with,
            text,
            ranges,
        }
    }

    /// The ranges, when `print` is their JSON and no text of the host's own
    /// replaced the document since: they stay where the editor moved them.
    pub(crate) fn again(&self, print: Option<&str>) -> Option<Vec<TextRange>> {
        (self.print.is_some() && self.print.as_deref() == print).then(|| self.ranges.clone())
    }

    /// The text they came with.
    pub(crate) fn came_with(&self) -> &str {
        &self.came_with
    }

    /// The editor was handed them followed to `text`, as `ranges`.
    pub(crate) fn followed(&mut self, text: &str, ranges: Vec<TextRange>) {
        self.text = if ranges.is_empty() {
            Rc::from("")
        } else {
            Rc::from(text)
        };
        self.ranges = ranges;
    }

    /// A text of the host's own replaces the document: the ranges to hand
    /// the editor there, carried from the given ones. The same ranges given
    /// next are read in the text they come with.
    /// The host holds that text now: ranges it gives next were worked out
    /// on it, not on the one the last ones came with.
    pub(crate) fn replaced(&mut self, text: &str) -> Vec<TextRange> {
        self.print = None;
        self.came(text);
        carried(&self.text, text, &self.ranges)
    }

    /// Ranges came with `text` (taken or not): the host's text the next ones
    /// may have been worked out on.
    pub(crate) fn came(&mut self, text: &str) {
        if !self.ranges.is_empty() && *self.came_with != *text {
            self.came_with = Rc::from(text);
        }
    }

    /// Ranges came that are not taken (for a text a reset replaced): the
    /// same ones given next are read in the text they come with.
    pub(crate) fn forget(&mut self) {
        self.print = None;
    }
}

/// Each listed document's [`HostLocks`], from the last list and from the
/// list being read (which replaces them once it is read whole); while the
/// view takes the list, the ones before it too.
#[derive(Default)]
pub(crate) struct Listed {
    last: HashMap<String, HostLocks>,
    read: HashMap<String, HostLocks>,
    before: HashMap<String, HostLocks>,
}

impl Listed {
    /// Document `id`'s, from the last list.
    #[cfg(test)]
    pub(crate) fn get(&self, id: &str) -> Option<&HostLocks> {
        self.last.get(id)
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut HostLocks> {
        self.last.get_mut(id)
    }

    /// Document `id` is listed with `value` and the ranges printed `print`:
    /// the ranges its editor is handed, and what became of ranges listed
    /// again as given last. Those stay where its editor moved them unless
    /// `value` replaces its text (`replaces`); else `read` reads them, in
    /// `value` (given the text the last ones came with, see
    /// `protect::ranges_in`).
    pub(crate) fn list<E>(
        &mut self,
        id: &str,
        print: Option<String>,
        value: &str,
        replaces: impl FnOnce() -> bool,
        read: impl FnOnce(Option<&str>) -> Result<Vec<TextRange>, E>,
    ) -> Result<(Vec<TextRange>, Option<Again>), E> {
        let last = self.last.get(id);
        let (ranges, again) = match last.and_then(|locks| locks.again(print.as_deref())) {
            Some(ranges) if stay(true, replaces()) => (ranges, Some(Again::Stayed)),
            again => (
                read(last.map(HostLocks::came_with))?,
                again.map(Again::Read),
            ),
        };
        let locks = match (&again, last) {
            (Some(Again::Stayed), Some(last)) => {
                let mut locks = last.clone();
                locks.came(value);
                locks
            }
            _ => HostLocks::given(print, value, value, ranges.clone()),
        };
        self.read.insert(id.to_string(), locks);
        Ok((ranges, again))
    }

    /// The view took document `id`'s listed text as an echo or not (see
    /// `echo_ranges::take`): its editor kept the ranges it had (`stayed`),
    /// was handed the ones read followed to its text `current` (`kept`),
    /// or the text came from before a reset the host has not given back
    /// (`before_reset`: not taken).
    pub(crate) fn took(
        &mut self,
        id: &str,
        current: &str,
        kept: Option<Vec<TextRange>>,
        stayed: bool,
        before_reset: bool,
    ) {
        if before_reset || stayed {
            self.restore(id, before_reset);
        } else if let (Some(ranges), Some(locks)) = (kept, self.last.get_mut(id)) {
            locks.followed(current, ranges);
        }
    }

    /// The list is read whole: what it read is kept for the next one, and
    /// the ones before it until the view took it (see [`Listed::restore`]).
    pub(crate) fn finish(&mut self) {
        self.before = std::mem::replace(&mut self.last, std::mem::take(&mut self.read));
    }

    /// The list could not be read: the last one's stay.
    pub(crate) fn abandon(&mut self) {
        self.read.clear();
    }

    /// Document `id`'s editor keeps what it had before the list (the text
    /// listed did not replace its own, or came from before a reset): its
    /// record from before the list, `forget` for ranges not taken.
    fn restore(&mut self, id: &str, forget: bool) {
        if let Some(mut before) = self.before.get(id).cloned() {
            if forget {
                before.forget();
            } else if let Some(listed) = self.last.get(id) {
                // They came with the text listed now (a reset's text stays
                // the host's until it gives one after it).
                before.came_with = listed.came_with.clone();
            }
            self.last.insert(id.to_string(), before);
        }
    }

    /// The view took the list.
    pub(crate) fn taken(&mut self) {
        self.before.clear();
    }
}

#[cfg(test)]
#[path = "host_locks/tests.rs"]
mod tests;
