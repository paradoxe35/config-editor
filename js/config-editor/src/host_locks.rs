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
//!   one task compose;
//! - the same ranges given again (the same JSON, no new host text: a
//!   framework's render, a list given again) stay where the editor moved
//!   them; after a text of the host's own they are a statement about that
//!   text, read in it;
//! - ranges that name places the text they come with lacks, but fit the
//!   text the last ones came with, were worked out on that one (a render
//!   listing a newer text with ranges from the one it rendered before)
//!   and are read there (`protect::ranges_in`); after a text of the
//!   host's own, the text they came with before it is such a text too (a
//!   host that resets and renders the ranges it had);
//! - ranges given with a text from before a reset the host has not given
//!   back yet (`Echoes::before_reset`) are for a text it replaced: they are
//!   not taken, and the carried ones stay.
//!
//! Every rule decides from this one record per document, which the
//! document's `HostChannel` holds beside its echoes and applies the rules
//! with; nothing else keeps the host's ranges.

use std::rc::Rc;

use config_editor_leptos::TextRange;
use wasm_bindgen::JsValue;

use crate::remap::{carried, moved_between};

/// What a list of ranges is compared by: its JSON. `None` when it has none
/// (a cycle, a value JSON cannot write): it then equals no other.
pub(crate) fn print(list: &JsValue) -> Option<String> {
    js_sys::JSON::stringify(list)
        .ok()
        .and_then(|text| text.as_string())
}

/// One document's host ranges: as given, in the text they name places in.
#[derive(Clone, Debug, Default)]
pub(crate) struct HostLocks {
    /// Their JSON, while ranges given again are compared with them: `None`
    /// once a text of the host's own replaced the document.
    print: Option<String>,
    /// The text they came with (where a host worked out the next ones,
    /// when those do not fit theirs).
    came_with: Rc<str>,
    /// The text they came with before a text of the host's own replaced
    /// the document: a host that resets and then renders the ranges it
    /// had may have worked them out on that one.
    before_reset: Option<Rc<str>>,
    /// The text their positions are in, and the positions: what the
    /// editor was handed from them.
    text: Rc<str>,
    ranges: Vec<TextRange>,
    /// Which handing of them this is: each new one (given, followed to a
    /// text, carried into a reset's) has its own, so the view places them
    /// anew even at the positions handed before (for another text).
    version: u64,
}

thread_local! {
    /// The last handing's version.
    static VERSIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A version no handing had.
fn next_version() -> u64 {
    VERSIONS.with(|last| {
        last.set(last.get() + 1);
        last.get()
    })
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
            before_reset: None,
            text,
            ranges,
            version: next_version(),
        }
    }

    /// Which handing of them this is (see `Document::read_only_ranges_version`).
    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    /// Whether `print` is their JSON and no text of the host's own replaced
    /// the document since: given again, they stay where the editor moved
    /// them.
    pub(crate) fn again(&self, print: Option<&str>) -> bool {
        self.print.is_some() && self.print.as_deref() == print
    }

    /// The texts the next ones may have been worked out on, when they do
    /// not fit theirs: the one they came with, then the one they came with
    /// before a text of the host's own replaced the document.
    pub(crate) fn came_with(&self) -> [&str; 2] {
        let came_with = &*self.came_with;
        [came_with, self.before_reset.as_deref().unwrap_or(came_with)]
    }

    /// A text of the host's own replaces the document: the ranges to hand
    /// the editor there, carried from the given ones. The host holds that
    /// text now, so the ranges it gives next (the same ones too) are read
    /// in the text they come with, worked out on it rather than on the one
    /// the last ones came with.
    pub(crate) fn replaced(&mut self, text: &str) -> Vec<TextRange> {
        self.print = None;
        self.version = next_version();
        if self.before_reset.is_none() && !self.ranges.is_empty() {
            self.before_reset = Some(self.came_with.clone());
        }
        self.came(text);
        carried(&self.text, text, &self.ranges)
    }

    /// An editor taken over on `value` (a framework re-mount) holds these
    /// ranges still: the ones to hand it there, moved from the text their
    /// positions are in as an edit moves them. Never read again in a text
    /// the host rendered them with since: an echo of the editor's own text
    /// rendered with them names other places with the same numbers.
    pub(crate) fn moved_to(&mut self, value: &str) -> Vec<TextRange> {
        let (ranges, _) = moved_between(&self.text, value, Some(self.ranges.clone()), None);
        self.ranges = ranges.unwrap_or_default();
        if !self.ranges.is_empty() {
            self.text = Rc::from(value);
        }
        self.version = next_version();
        self.ranges.clone()
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

#[cfg(test)]
#[path = "host_locks/tests.rs"]
mod tests;
