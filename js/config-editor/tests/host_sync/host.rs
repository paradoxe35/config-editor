//! The host of a simulated view: per document, the text it stores (each
//! `onChange` text once it comes back from its store, late) with its
//! revision, the texts on their way back to the editor, and the texts it
//! listed and gave last. A host that keeps revisions (`revisions`) keeps
//! the newest one: an older text coming back late is not rendered.

use crate::model::{Class, LOCKED, ResetKind, new_version};

/// A text on its way back to the editor.
#[derive(Clone, Debug)]
pub(crate) struct Queued {
    pub(crate) text: String,
    /// A reset's listing (the host renders its own texts in order).
    pub(crate) listing: bool,
    /// The revision `onChange` gave it with.
    pub(crate) revision: Option<u64>,
    /// Sent by an editor that a hide and show replaced.
    pub(crate) before_fresh: bool,
}

pub(crate) struct HostDoc {
    /// What the host holds, and the revision it holds it at.
    pub(crate) store: String,
    pub(crate) store_revision: Option<u64>,
    /// Texts on their way back.
    pub(crate) queue: Vec<Queued>,
    /// The text it listed (or reset to) last.
    pub(crate) last_listed: String,
    /// The text it started from.
    pub(crate) original: String,
    /// The texts it reset to, and of those the ones the editor held then.
    pub(crate) resets: Vec<String>,
    pub(crate) same_resets: Vec<String>,
    /// The text it gave the editor last in an update or a list.
    last_given: String,
    /// Whether the host holds a text of its own that is the one it gave
    /// last, given again (see [`HostDoc::own`]).
    pub(crate) holds_given_again: bool,
    /// Whether it keeps the newest revision (passes `valueRevision`).
    pub(crate) revisions: bool,
}

impl HostDoc {
    pub(crate) fn new(text: &str, revisions: bool) -> Self {
        Self {
            store: text.to_string(),
            store_revision: None,
            queue: Vec::new(),
            last_listed: text.to_string(),
            original: text.to_string(),
            resets: Vec::new(),
            same_resets: Vec::new(),
            last_given: text.to_string(),
            holds_given_again: false,
            revisions,
        }
    }

    /// An `onChange` text, on its way back.
    pub(crate) fn told(&mut self, text: &str, revision: Option<u64>) {
        self.queue.push(Queued {
            text: text.to_string(),
            listing: false,
            revision,
            before_fresh: false,
        });
    }

    /// The host gives the editor `text` in an update or a list.
    pub(crate) fn gave(&mut self, text: &str) {
        text.clone_into(&mut self.last_given);
    }

    /// The host stores `text` at `revision` (an echo, or a reset's text):
    /// with revisions, only a newer one (it never goes back). Whether it
    /// stored it.
    pub(crate) fn hold(&mut self, text: &str, revision: Option<u64>) -> bool {
        if self.revisions
            && let (Some(old), Some(new)) = (self.store_revision, revision)
            && new < old
        {
            return false;
        }
        if self.store != text {
            text.clone_into(&mut self.store);
            self.holds_given_again = false;
        }
        self.store_revision = revision.filter(|_| self.revisions);
        true
    }

    /// The host gives `text` of its own (`update({ value })`), its pending
    /// echoes dropped. A text that is the one it gave last is, by the
    /// echo contract, that text given again: no news.
    pub(crate) fn own(&mut self, text: &str) {
        self.queue.clear();
        text.clone_into(&mut self.store);
        self.store_revision = None;
        self.holds_given_again = text == self.last_given;
    }

    /// A reset's listing (at the reset's revision, for a host keeping
    /// them), on its way.
    pub(crate) fn lists(&mut self, text: &str, revision: Option<u64>) {
        self.queue.push(Queued {
            text: text.to_string(),
            listing: true,
            revision,
            before_fresh: false,
        });
    }

    /// Takes the queued text at `pick`.
    pub(crate) fn take(&mut self, pick: u16) -> Option<Queued> {
        if self.queue.is_empty() {
            return None;
        }
        let at = pick as usize % self.queue.len();
        let queued = self.queue.remove(at);
        if queued.listing {
            let mut index = 0;
            self.queue.retain(|older| {
                index += 1;
                !(older.listing && index <= at)
            });
        }
        Some(queued)
    }

    /// The host lists its store: the listings still queued are older.
    pub(crate) fn listed_store(&mut self) {
        self.queue.retain(|queued| !queued.listing);
    }

    /// The queued text at `pick`, left queued.
    pub(crate) fn peek(&self, pick: u16) -> Option<&Queued> {
        (!self.queue.is_empty()).then(|| &self.queue[pick as usize % self.queue.len()])
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

    /// A hide and show: the texts still on their way were sent by the
    /// editor that is gone.
    pub(crate) fn fresh_mount(&mut self) {
        for queued in &mut self.queue {
            queued.before_fresh = true;
        }
    }

    /// The texts a reset of `kind` gives, in order, the editor holding
    /// `shown`.
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

    /// It reset to `text`, the editor holding `shown`.
    pub(crate) fn reset(&mut self, text: &str, shown: &str) {
        self.resets.push(text.to_string());
        if text == shown {
            self.same_resets.push(text.to_string());
        }
    }

    /// The class of a text handed back that replaced newer typing.
    pub(crate) fn lost_by(&self, queued: Option<&Queued>, text: &str) -> Class {
        if queued.is_some_and(|queued| queued.before_fresh) {
            Class::TypingLostAfterHideShow
        } else if self.same_resets.iter().any(|reset| reset == text) {
            Class::RevertedBySameListing
        } else if self.resets.iter().any(|reset| reset == text) {
            Class::RevertedByListing
        } else {
            Class::TypingLost
        }
    }
}
