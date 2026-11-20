//! One document's channel to its host: the texts it sent that the host has
//! not handed back yet (the core's `Echoes`) and the host's read-only
//! ranges as given (`HostLocks`), held together, so "the host gave a new
//! value, ranges or diagnostics" is decided in one place for a
//! `ConfigEditor` (one channel) and for each document of a
//! `ConfigDocuments` (one channel per document).
//!
//! An update goes in two steps, so a bad list throws before anything
//! changes: [`HostChannel::ranges`] reads the ranges given (or decides
//! they stay, unread), and [`HostChannel::received`] then takes the whole
//! update and says what the editor shows. The second step reads no
//! JavaScript: it is the same code the native random suite drives
//! (`tests/host_sync`).
//!
//! By path: the native tests (`tests/host_channel.rs`, `tests/host_sync`)
//! load this file too.

use config_editor_leptos::js_support::{Echo, Echoes};
use config_editor_leptos::{HostDiagnostic, TextRange};

use crate::host_locks::HostLocks;
use crate::remap::{moved_between, reads_as};

/// What the host gave a document in one update: an editor's `update`, or
/// the document's entry in a `documents` list.
pub(crate) struct Given<'a> {
    /// Its text, if it gave one.
    pub(crate) value: Option<&'a str>,
    /// The revision `onChange` sent `value` with, passed back
    /// (`valueRevision`): its echo is matched exactly.
    pub(crate) revision: Option<u64>,
    /// The text the ranges and diagnostics name places in, when not
    /// `value` (`rangesFor`).
    pub(crate) ranges_for: Option<&'a str>,
    /// Its `readOnlyRanges`, as [`HostChannel::ranges`] read them.
    pub(crate) ranges: Option<Ranges>,
    /// Its host diagnostics, read in the text they were given for.
    pub(crate) diagnostics: Option<Vec<HostDiagnostic>>,
    /// The texts sent from this point of the clock on were sent while the
    /// update was handled (an edit still held, sent first): the host did
    /// not know them.
    pub(crate) flushed: u64,
    /// `value` is the text the host was told last, and that flush sent it
    /// a newer one: it is older than what the user typed.
    pub(crate) stale: bool,
}

/// The `readOnlyRanges` of an update, read (or not) by [`HostChannel::ranges`].
pub(crate) struct Ranges {
    /// Their JSON (what ranges given again are compared by).
    print: Option<String>,
    /// The ranges given last, given again.
    again: bool,
    /// The ranges, read in the text they name places in; `None` when they
    /// are the ones given last and stay where the editor moved them.
    read: Option<Vec<TextRange>>,
}

/// What became of an update (see [`HostChannel::received`]).
#[derive(Debug)]
pub(crate) struct Taken {
    /// What the echo engine made of its text; `None` without one.
    pub(crate) echo: Option<Echo>,
    /// The ranges to hand the editor, in the text it shows after the
    /// update; `None`: it keeps the ones it has, where it moved them.
    pub(crate) ranges: Option<Vec<TextRange>>,
    /// The diagnostics given, in that text.
    pub(crate) diagnostics: Option<Vec<HostDiagnostic>>,
}

#[cfg(test)]
impl Taken {
    /// Whether the update's text replaces the document (or is it).
    pub(crate) fn shows(&self) -> bool {
        self.echo == Some(Echo::Apply)
    }
}

/// One document's channel to its host (see the module).
#[derive(Debug)]
pub(crate) struct HostChannel {
    echoes: Echoes,
    locks: HostLocks,
}

impl HostChannel {
    /// A document the host gave as `value`, with `ranges` (printed `print`)
    /// read as positions in `value`, given for `came_with`; `echoing`: the
    /// host hands every `onChange` text back.
    pub(crate) fn new(
        echoing: bool,
        value: &str,
        print: Option<String>,
        came_with: &str,
        ranges: Vec<TextRange>,
    ) -> Self {
        let mut echoes = Echoes::new(echoing);
        echoes.given(value);
        Self {
            echoes,
            locks: HostLocks::given(print, came_with, value, ranges),
        }
    }

    /// A new editor takes this channel over from the one it replaces (a
    /// framework re-mount), its document `value` and its host ranges as
    /// [`HostChannel::new`] takes them: the texts still on their way are
    /// echoes for it too. A `value` that is not the text sent last (a
    /// reset made while there was no editor) is the host's own, on its
    /// way as a reset is.
    pub(crate) fn remount(
        &mut self,
        value: &str,
        print: Option<String>,
        came_with: &str,
        ranges: Vec<TextRange>,
    ) {
        self.echoes.carried(value);
        self.locks = HostLocks::given(print, came_with, value, ranges);
    }

    /// The first step of an update: its `readOnlyRanges` (printed `print`),
    /// given with `value` or for `ranges_for`, the document being
    /// `current`. The ranges given last, given again, stay where the
    /// editor moved them unless the text they come with replaces the
    /// document: they are not read at all then, so they are never checked
    /// against a text they were not given for (an echo, whose lines may be
    /// shorter). Else `read` reads them in the text they name places in,
    /// given the text the last ones came with (where a host may have
    /// worked them out, see `protect::ranges_in`).
    pub(crate) fn ranges<E>(
        &mut self,
        print: Option<String>,
        value: Option<&str>,
        ranges_for: Option<&str>,
        current: &str,
        stale: bool,
        read: impl FnOnce(&str, &[&str]) -> Result<Vec<TextRange>, E>,
    ) -> Result<Ranges, E> {
        let again = ranges_for.is_none() && self.locks.again(print.as_deref());
        let replaces = value.is_some_and(|value| {
            !(stale && value != current) && self.echoes.replaces(value, current)
        });
        let read = if again && !replaces {
            None
        } else {
            let host_text = ranges_for.or(value).unwrap_or(current);
            Some(read(host_text, &self.locks.came_with())?)
        };
        Ok(Ranges { print, again, read })
    }

    /// The second step: takes `given`, the document being `current`.
    ///
    /// - Its text, an echo the user typed past or older than the typing,
    ///   keeps the document; else it replaces it (see `Echoes`).
    /// - Ranges given again with a text that keeps the document stay
    ///   where the editor moved them.
    /// - Ranges given with a text from before the host's last reset, which
    ///   it has not given back yet, are for a text the reset replaced: the
    ///   editor keeps the ones carried into the reset's text, and the same
    ///   ones given next are read in their text.
    /// - Ranges and diagnostics given for another text than the document
    ///   (an echo, `rangesFor`) are moved to it.
    /// - A text of the host's own that replaces the document with no ranges
    ///   hands the editor the given ones carried into it.
    pub(crate) fn received(&mut self, given: Given<'_>, current: &str) -> Taken {
        let Given {
            value,
            revision,
            ranges_for,
            ranges,
            mut diagnostics,
            flushed,
            stale,
        } = given;
        let echo = value.map(|value| {
            if stale && value != current {
                return Echo::Skip;
            }
            match revision {
                Some(revision) => self.echoes.received_at(value, revision, current, flushed),
                None => self.echoes.received(value, current, flushed),
            }
        });
        let shown = value.filter(|_| echo == Some(Echo::Apply));
        let before_reset =
            shown.is_none() && value.is_some_and(|value| self.echoes.before_reset(value));
        let ranges_given = ranges.is_some();
        let (print, again, mut read) = match ranges {
            Some(Ranges { print, again, read }) => (print, again, read),
            None => (None, false, None),
        };
        if before_reset || (again && shown.is_none()) {
            read = None;
        }
        let document = shown.unwrap_or(current);
        let host_text = ranges_for.or(value).unwrap_or(document);
        // An empty list is the same in every text: no diff for it.
        let placed = read.as_ref().is_some_and(|ranges| !ranges.is_empty())
            || diagnostics.as_ref().is_some_and(|list| !list.is_empty());
        if placed && host_text != document {
            (read, diagnostics) = moved_between(host_text, document, read, diagnostics);
        }
        if before_reset {
            self.locks.forget();
        } else if ranges_given && read.is_none() {
            // Given again, they stayed: they came with this text.
            self.locks.came(host_text);
        } else if let Some(ranges) = &read {
            self.locks = HostLocks::given(print, host_text, document, ranges.clone());
        } else if let Some(value) = shown.filter(|value| !reads_as(value, current)) {
            read = Some(self.locks.replaced(value));
        }
        Taken {
            echo,
            ranges: read,
            diagnostics,
        }
    }

    /// The host gave `value` of its own (`replaceValue`), the document
    /// being `current`: what it holds, no echo to wait for. Returns the
    /// ranges to hand the editor there, carried from the ones given; `None`
    /// when `value` is `current` as the editor reads it.
    pub(crate) fn replaced(&mut self, value: &str, current: &str) -> Option<Vec<TextRange>> {
        self.echoes.replaced(value);
        (!reads_as(value, current)).then(|| self.locks.replaced(value))
    }

    /// The editor sent `text` to the host (`onChange`): its revision.
    pub(crate) fn sent(&mut self, text: &str) -> u64 {
        self.echoes.sent(text)
    }

    /// The document changed and the host was not told (no `onChange`).
    pub(crate) fn unsent(&mut self) {
        self.echoes.unsent();
    }

    /// The pause after an echo left the host on an older text is over:
    /// whether the host is to hear `current` again (see `Echoes::settle`).
    pub(crate) fn settle(&mut self, current: &str) -> bool {
        self.echoes.settle(current)
    }

    /// The revision of the text sent last, if any is on its way.
    pub(crate) fn latest_revision(&self) -> Option<u64> {
        self.echoes.latest_revision()
    }

    /// Which handing of the ranges the editor has (see
    /// `Document::read_only_ranges_version`).
    pub(crate) fn version(&self) -> u64 {
        self.locks.version()
    }
}

#[cfg(test)]
#[path = "host_channel/tests.rs"]
mod tests;
