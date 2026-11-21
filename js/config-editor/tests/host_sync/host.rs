//! The host of a simulated view: per document, the text it stores (each
//! `onChange` text once it comes back from its store, late), the texts on
//! their way back to the editor, and the texts it listed and gave last.

use crate::model::{Class, Form, LOCKED, ResetKind, host_lock, new_version};
use leptos_config_editor::TextRange;

pub(crate) struct HostDoc {
    /// What the host holds.
    pub(crate) store: String,
    /// Texts on their way back (each `onChange` text, and each reset's
    /// listing, marked `true`), handed back in any order but for the
    /// listings: a host renders its own texts in order, so a listing
    /// handed back drops the older ones still queued.
    pub(crate) queue: Vec<(String, bool)>,
    /// The text it listed (or reset to) last.
    pub(crate) last_listed: String,
    /// The text it started from.
    pub(crate) original: String,
    /// The ranges it gave last, as compared (their JSON).
    pub(crate) last_print: String,
    /// The texts it gave `replaceValue`, and of those the ones the
    /// editor held then.
    pub(crate) resets: Vec<String>,
    pub(crate) same_resets: Vec<String>,
    /// The text it gave the editor last in an update or a list (a reset's
    /// text is given by `replaceValue`, and listed later).
    last_given: String,
    /// Whether the host holds a text of its own that is the one it gave
    /// last, given again (see [`HostDoc::own`]).
    pub(crate) holds_given_again: bool,
}

impl HostDoc {
    pub(crate) fn new(text: &str) -> Self {
        Self {
            store: text.to_string(),
            queue: Vec::new(),
            last_listed: text.to_string(),
            original: text.to_string(),
            last_print: String::new(),
            resets: Vec::new(),
            same_resets: Vec::new(),
            last_given: text.to_string(),
            holds_given_again: false,
        }
    }

    /// An `onChange` text, on its way back.
    pub(crate) fn told(&mut self, text: &str) {
        self.queue.push((text.to_string(), false));
    }

    /// The host gives the editor `text` in an update or a list.
    pub(crate) fn gave(&mut self, text: &str) {
        text.clone_into(&mut self.last_given);
    }

    /// The host stores `text` (an echo, or a reset's text).
    pub(crate) fn hold(&mut self, text: &str) {
        if self.store != text {
            text.clone_into(&mut self.store);
            self.holds_given_again = false;
        }
    }

    /// The host gives `text` of its own (`update({ value })`), its pending
    /// echoes dropped (a reset's listing still on its way among them). A
    /// text that is the one it gave last is, by the echo contract, that
    /// text given again (a framework's render passing an unchanged prop):
    /// no news, so the editor keeps its document (the reset's text), and
    /// the two may differ at rest. It happens only after a reset: else
    /// the host holds the text it gave last, and a new version differs.
    pub(crate) fn own(&mut self, text: &str) {
        self.queue.clear();
        text.clone_into(&mut self.store);
        self.holds_given_again = text == self.last_given;
    }

    /// A reset's listing, on its way.
    pub(crate) fn lists(&mut self, text: &str) {
        self.queue.push((text.to_string(), true));
    }

    /// Takes the queued text at `pick`, and whether it is a listing.
    pub(crate) fn take(&mut self, pick: u16) -> Option<(String, bool)> {
        if self.queue.is_empty() {
            return None;
        }
        let at = pick as usize % self.queue.len();
        let (text, listing) = self.queue.remove(at);
        if listing {
            let mut index = 0;
            self.queue.retain(|(_, older)| {
                index += 1;
                !(*older && index <= at)
            });
        }
        Some((text, listing))
    }

    /// The host lists its store: the listings still queued are older.
    pub(crate) fn listed_store(&mut self) {
        self.queue.retain(|(_, listing)| !listing);
    }

    /// The text at `pick` fails to come back now and is sent again (it
    /// comes after the others).
    pub(crate) fn retry(&mut self, pick: u16) {
        if !self.queue.is_empty() {
            let at = pick as usize % self.queue.len();
            let entry = self.queue.remove(at);
            self.queue.push(entry);
        }
    }

    /// The texts a `replaceValue` of `kind` gives, in order (two for
    /// [`ResetKind::Twice`]), the editor holding `shown`.
    pub(crate) fn resets(&self, kind: ResetKind, shown: &str) -> Vec<String> {
        match kind {
            ResetKind::NewVersion(edit) => vec![new_version(&self.store, edit)],
            ResetKind::Same => vec![shown.to_string()],
            ResetKind::LastListed => vec![self.last_listed.clone()],
            ResetKind::Original => vec![self.original.clone()],
            ResetKind::Twice(edit) => {
                let first = new_version(&self.store, edit);
                let second = new_version(&first, edit.wrapping_add(7));
                vec![first, second]
            }
        }
        .into_iter()
        // A host keeps its locked text (the lock is its promise).
        .filter(|text| text.contains(LOCKED))
        .collect()
    }

    /// It gives `replaceValue` `text`, the editor holding `shown`.
    pub(crate) fn reset(&mut self, text: &str, shown: &str) {
        self.resets.push(text.to_string());
        if text == shown {
            self.same_resets.push(text.to_string());
        }
    }

    /// The class of a text handed back that replaced newer typing.
    pub(crate) fn lost_by(&self, text: &str) -> Class {
        if self.same_resets.iter().any(|reset| reset == text) {
            Class::RevertedBySameListing
        } else if self.resets.iter().any(|reset| reset == text) {
            Class::RevertedByListing
        } else {
            Class::TypingLost
        }
    }

    /// Whether the host gives ranges with `text`: when asked (`wanted`),
    /// and always when they are not the ones it gave last (a host leaving
    /// them out keeps the ones it gave, as a framework's unchanged prop).
    pub(crate) fn gives_ranges(&mut self, text: &str, form: Form, wanted: bool) -> bool {
        let (_, print) = ranges_in(text, form);
        let gives = wanted || print != self.last_print;
        if gives {
            self.last_print = print;
        }
        gives
    }
}

/// The host's ranges in `text`, as it works them out in its copy.
pub(crate) fn ranges_in(text: &str, form: Form) -> (Vec<TextRange>, String) {
    match host_lock(text, form) {
        Some((range, print)) => (vec![range], print),
        None => (Vec::new(), "[]".to_string()),
    }
}
