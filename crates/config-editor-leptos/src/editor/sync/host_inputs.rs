//! What the host gives a document, through one entry: a [`HostUpdate`]
//! (a text, the revision it came with, read-only ranges and diagnostics,
//! each with the text it names places in), and the one record that decides
//! it: the echo engine, the host log and the lists as given, held together
//! in a [`HostChannel`] that a re-mount passes on.
//!
//! Where a list's numbers are read is decided by the rules of
//! [`Link::anchor_of`], from facts the channel recorded (the echo engine's
//! decision, the host log), never from whether the list looks changed:
//!
//! 1. an anchor that is a revision names the text sent at it (or this
//!    update's `value`, given with that revision): mapped to the document
//!    through the log, exactly; a revision the log no longer keeps is out of
//!    date (the list is not taken, the locks stay), one it never issued is
//!    refused;
//! 2. an anchor that is a text names that text: the version of the log
//!    with that text when exactly one has it, else the text is moved to
//!    the document by the safe diff (`carry`);
//! 3. no anchor, with a `value`: that value (an own text read in itself,
//!    an echo through the log);
//! 4. no anchor, no value: the host's text now;
//! 5. a list with no anchor equal to the one in force, given with no own
//!    text, is the same statement again: nothing changes.
//!
//! The given ranges are kept in bytes of the version they name, with the
//! text each holds there; what the screen locks is derived from them
//! (mapped through the log to the document, see [`Link::placed`]).

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use super::echoes::{Echo, Echoes, Handoff, Print, issue};
use super::host_log::{HostLog, Named, Rewrite};
use crate::editor::host_diagnostics::{DiagnosticTarget, HostDiagnostic};
use crate::editor::paths;
use crate::editor::text::{TextRange, TextSize, slice, text_size};

pub(crate) mod carry;

use carry::Lines;

/// Which text a list of positions (read-only ranges, diagnostics) names
/// places in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangesFor {
    /// The text `on_change` sent with this revision (or the host's own
    /// text it gave with it as `value_revision`).
    Revision(u64),
    /// This text: one with no revision (the file as loaded).
    Text(Arc<str>),
}

/// A list as the host gave it, and what it is compared by when given again
/// (rule 5): the caller's statement (a JavaScript list's JSON) or, without
/// one, its items.
#[derive(Clone, Debug, PartialEq)]
pub struct Listed<T> {
    /// The positions, in the text the list names (see [`Link::reading`]).
    pub items: Vec<T>,
    /// What the list is compared by when given again, if not its items.
    pub statement: Option<u64>,
}

impl<T: PartialEq> Listed<T> {
    /// `items`, compared by themselves.
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            statement: None,
        }
    }

    /// A list compared by `statement` (its items are read only when it is
    /// news).
    pub fn stated(items: Vec<T>, statement: u64) -> Self {
        Self {
            items,
            statement: Some(statement),
        }
    }

    fn same(&self, other: &Self) -> bool {
        match (self.statement, other.statement) {
            (Some(left), Some(right)) => left == right,
            (None, None) => self.items == other.items,
            _ => false,
        }
    }
}

/// The hash of a text (a list's JSON): what a list given by statement is
/// compared by.
pub fn statement_of(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// One update from the host (see the module).
#[derive(Clone, Debug, Default)]
pub struct HostUpdate {
    /// A new text, if it gave one.
    pub value: Option<String>,
    /// The revision `on_change` sent `value` with, passed back.
    pub value_revision: Option<u64>,
    /// Its read-only ranges, if it gave them.
    pub ranges: Option<Listed<paths::TextRange>>,
    /// The text the ranges name places in, when not the rule's default.
    pub ranges_for: Option<RangesFor>,
    /// Its diagnostics, if it gave them.
    pub diagnostics: Option<Listed<HostDiagnostic>>,
    /// The text the diagnostics name places in; left out, `ranges_for`.
    pub diagnostics_for: Option<RangesFor>,
    /// The texts sent from this point of the clock on were sent while the
    /// update was handled (an edit still held, sent first): the host did
    /// not know them. `None`: none was.
    pub flushed: Option<u64>,
    /// `value` is the text the host was told last, and the flush before
    /// the update sent it a newer one: it is older than what was typed.
    pub stale: bool,
    /// `value` is the host's own whatever it is (`replaceValue`): no echo,
    /// recorded as sent so its late listing is told apart.
    pub reset: bool,
}

/// Which list of an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    /// The read-only ranges.
    Ranges,
    /// The host diagnostics.
    Diagnostics,
}

/// Why a list's anchor cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnchorError {
    /// A revision issued that the editor does not keep: the host passed
    /// back a newer one or gave a text of its own since, or another editor
    /// (one a hide and show replaced) sent it.
    #[allow(missing_docs, reason = "the list and the revision it names")]
    OutOfDate { list: List, revision: u64 },
    /// A revision no editor on the page issued.
    #[allow(missing_docs, reason = "the list and the revision it names")]
    Unknown { list: List, revision: u64 },
}

impl AnchorError {
    fn names(list: List) -> (&'static str, &'static str) {
        match list {
            List::Ranges => ("readOnlyRanges", "rangesFor"),
            List::Diagnostics => ("hostDiagnostics", "diagnosticsFor"),
        }
    }

    /// Whether the list is only dropped (a warning), not refused: an out
    /// of date anchor, or any anchor of diagnostics (no lock).
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            AnchorError::OutOfDate { .. }
                | AnchorError::Unknown {
                    list: List::Diagnostics,
                    ..
                }
        )
    }
}

impl std::fmt::Display for AnchorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            AnchorError::OutOfDate { list, revision } => {
                let (name, anchor) = Self::names(list);
                write!(
                    formatter,
                    "{name} for revision {revision} are out of date: give them for that revision or a later one ({anchor})"
                )
            }
            AnchorError::Unknown { list, revision } => {
                let (_, anchor) = Self::names(list);
                write!(
                    formatter,
                    "{anchor} names revision {revision}, which no editor sent: give the revision onChange sent the text with, or the text itself"
                )
            }
        }
    }
}

impl std::error::Error for AnchorError {}

/// The text a list's numbers are in, for reading it (see [`Link::reading`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    /// The list in force, given again: nothing to read.
    Again,
    /// This text.
    In(Arc<str>),
}

/// Where a list is read (see [`Link::anchor_of`]).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Anchor {
    /// The list in force, given again.
    Again,
    /// A version the log keeps.
    Kept(u64),
    /// This update's value, wherever the echo engine puts it.
    Value,
    /// A text from outside, moved to the document by the safe diff.
    Outside(Arc<str>),
}

/// The ranges given, in bytes of the version of the log they name, and the
/// text each holds there.
#[derive(Clone, Debug, Default)]
struct Given {
    revision: u64,
    ranges: Vec<TextRange>,
    texts: Vec<String>,
}

/// What an update did (see [`Link::take`]).
#[derive(Debug, Default)]
pub struct Taken {
    /// What the echo engine made of its value; `None` without one.
    pub echo: Option<Echo>,
    /// Its value is a text of the host's own (no echo of a text sent, nor
    /// the text the host gave last given again), shown or the document
    /// already: written in the host's own form, whatever the document's.
    pub own: bool,
    /// Its value replaces the document (a text of the host's own).
    pub shown: bool,
    /// The given ranges changed: the screen places them again.
    pub ranges: bool,
    /// The diagnostics given, in the document (the log's), when new.
    pub diagnostics: Option<Vec<HostDiagnostic>>,
    /// Lists not taken, and why (shown as warnings).
    pub warnings: Vec<AnchorError>,
}

/// One document's record of its host (see the module).
#[derive(Debug)]
pub(crate) struct Link {
    pub(crate) echoes: Echoes,
    pub(crate) log: HostLog,
    given: Option<Given>,
    /// The ranges list in force as given, while it is a statement: carried
    /// into a text of the host's own, it is none.
    ranges_listed: Option<(Listed<paths::TextRange>, Option<RangesFor>)>,
    /// The same for diagnostics.
    diagnostics_listed: Option<(Listed<HostDiagnostic>, Option<RangesFor>)>,
    /// The statement of the ranges list a text of the host's own carried
    /// the locks of (no list came with it), until a list is given: that
    /// list rendered again with numbers past the new text's end is no
    /// news (its locks are the carried ones), not an error.
    carried_list: Option<u64>,
    /// A reset of the host's own came since the list in force was given:
    /// that list rendered again with an older copy of the text is no news,
    /// and with the reset's own text it is read in it.
    after_reset: bool,
    /// The diagnostics given last, in the log's document as it was then.
    pub(crate) diagnostics: Vec<HostDiagnostic>,
}

impl Link {
    /// A document the host gave as `value` (at `revision`, when it gave
    /// one); `echoing`: the host hands every `on_change` text back;
    /// `handed`: the copies an editor that showed this document before
    /// sent and the host has not handed back (see [`Echoes::handed`]).
    pub(crate) fn new(
        echoing: bool,
        value: &str,
        revision: Option<u64>,
        handed: Option<Handoff>,
    ) -> Self {
        let echoes = match handed.filter(|_| echoing) {
            Some(handed) => Echoes::handed(handed, value, revision),
            None => {
                let mut echoes = Echoes::new(echoing);
                echoes.given(value);
                echoes
            }
        };
        let revision = revision.unwrap_or_else(issue);
        let mut log = HostLog::new(Arc::from(value), revision, echoing);
        log.printed_as(revision, Print::of(value));
        Self {
            echoes,
            log,
            given: None,
            ranges_listed: None,
            diagnostics_listed: None,
            carried_list: None,
            after_reset: false,
            diagnostics: Vec::new(),
        }
    }

    /// The host hands every `on_change` text back (`echoes`), the document
    /// being `text`: from now on texts sent are recorded (a record that
    /// does already stays).
    pub(crate) fn echoing(&mut self, text: &str) {
        if !self.echoes.is_on() {
            self.echoes = Echoes::new(true);
            self.echoes.given(text);
            self.log.set_confirms(true);
        }
    }

    /// A new editor takes this record over with `text` as its document (a
    /// framework re-mounting its component). A text other than the
    /// document (a reset the host made while there was no editor) is the
    /// host's own, on its way as a reset's is: the ranges are carried into
    /// it.
    pub(crate) fn remounted(&mut self, text: &str, rewrite: Rewrite<'_>) {
        self.echoes.carried(text);
        if **self.log.document() == *text {
            return;
        }
        let before = self.placed_now(rewrite);
        let revision = self.echoes.last_sent().unwrap_or_else(issue);
        self.log.own(Arc::from(text), revision);
        self.log.printed_as(revision, Print::of(text));
        self.carry(before, revision);
    }

    /// Whether ranges are given.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "protection reads it")
    )]
    pub(crate) fn has_ranges(&self) -> bool {
        self.given
            .as_ref()
            .is_some_and(|given| !given.ranges.is_empty())
    }

    /// Whether `update`'s value would replace the document `current` as a
    /// text of the host's own (not an echo, not the text already).
    fn replaces(&mut self, update: &HostUpdate, current: &str) -> bool {
        let Some(value) = update.value.as_deref() else {
            return false;
        };
        if update.stale && value != current {
            return false;
        }
        match update.value_revision {
            Some(revision) => self.echoes.replaces_at(value, revision, current),
            None => self.echoes.replaces(value, current),
        }
    }

    /// Where `update`'s `list` is read, the document being `current` (the
    /// rules of the module, top to bottom).
    fn anchor_of(
        &mut self,
        list: List,
        update: &HostUpdate,
        current: &str,
    ) -> Result<Anchor, AnchorError> {
        let anchor = match list {
            List::Ranges => update.ranges_for.as_ref(),
            List::Diagnostics => update
                .diagnostics_for
                .as_ref()
                .or(update.ranges_for.as_ref()),
        };
        // The list in force, naming the same text: the same statement.
        let again = match list {
            List::Ranges => update
                .ranges
                .as_ref()
                .zip(self.ranges_listed.as_ref())
                .is_some_and(|(given, (listed, named))| {
                    given.same(listed) && named.as_ref() == anchor
                }),
            List::Diagnostics => update
                .diagnostics
                .as_ref()
                .zip(self.diagnostics_listed.as_ref())
                .is_some_and(|(given, (listed, named))| {
                    given.same(listed) && named.as_ref() == anchor
                }),
        };
        let reset_listed =
            self.after_reset && update.value.as_deref().is_none_or(|value| value == current);
        if again && !reset_listed && !self.replaces(update, current) {
            return Ok(Anchor::Again);
        }
        match anchor {
            None if update.value.is_some() => Ok(Anchor::Value),
            None => Ok(Anchor::Kept(self.log.host_revision())),
            Some(RangesFor::Revision(revision)) => {
                let named = self.log.named(*revision);
                // The revision the value comes with names the value, known to
                // the editor or not; but a value the host confirmed past is
                // an old render (out of date).
                if update.value.is_some()
                    && update.value_revision == Some(*revision)
                    && named != Named::Kept
                    && (named != Named::OutOfDate || self.replaces(update, current))
                {
                    return Ok(Anchor::Value);
                }
                match named {
                    Named::Kept => Ok(Anchor::Kept(*revision)),
                    Named::OutOfDate => Err(AnchorError::OutOfDate {
                        list,
                        revision: *revision,
                    }),
                    Named::Unknown => Err(AnchorError::Unknown {
                        list,
                        revision: *revision,
                    }),
                }
            }
            Some(RangesFor::Text(text)) => Ok(match self.log.printed(Print::of(text)) {
                Some(revision) => Anchor::Kept(revision),
                None => Anchor::Outside(text.clone()),
            }),
        }
    }

    /// The text `update`'s `list` names places in, to read its numbers in,
    /// before anything changes: [`Reading::Again`] for the list in force
    /// given again. An anchor that cannot be read is an error (see
    /// [`AnchorError::is_warning`]).
    pub(crate) fn reading(
        &mut self,
        list: List,
        update: &HostUpdate,
        current: &str,
    ) -> Result<Reading, AnchorError> {
        Ok(match self.anchor_of(list, update, current)? {
            Anchor::Again => Reading::Again,
            Anchor::Value => Reading::In(Arc::from(update.value.as_deref().unwrap_or_default())),
            Anchor::Outside(text) => Reading::In(text),
            Anchor::Kept(revision) => Reading::In(
                self.log
                    .text_at(revision)
                    .unwrap_or_else(|| self.log.document().clone()),
            ),
        })
    }

    /// Takes `update`, the host's text now being `current` (see the
    /// module); nothing changes when it returns an error (a list's
    /// anchor the editor never issued). `flushed`: see
    /// [`HostUpdate::flushed`].
    pub(crate) fn take(
        &mut self,
        update: HostUpdate,
        current: &str,
        rewrite: Rewrite<'_>,
    ) -> Result<Taken, AnchorError> {
        let mut taken = Taken::default();
        if update.reset
            && let Some(value) = update.value.as_deref()
        {
            taken.shown = self.replaced(value, current, rewrite);
            taken.ranges = taken.shown;
            return Ok(taken);
        }
        let mut anchor = |link: &mut Self, list: List, given: bool| {
            if !given {
                return Ok(None);
            }
            match link.anchor_of(list, &update, current) {
                Ok(anchor) => Ok(Some(anchor)),
                Err(error) if error.is_warning() => {
                    taken.warnings.push(error);
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        };
        let ranges_anchor = anchor(self, List::Ranges, update.ranges.is_some())?;
        let diagnostics_anchor = anchor(self, List::Diagnostics, update.diagnostics.is_some())?;
        // A kept version a list names, as it reads now: a text of the
        // host's own shown below starts the log again without it, and the
        // list is then moved from that text to the document by the diff.
        let named_text = |link: &mut Self, anchor: &Option<Anchor>| match anchor {
            Some(Anchor::Kept(revision)) => link.log.text_at(*revision),
            _ => None,
        };
        let ranges_text = named_text(self, &ranges_anchor);
        let diagnostics_text = named_text(self, &diagnostics_anchor);
        // The ranges as the document holds them now, for a text of the
        // host's own that comes with none (carried into it).
        let before = (update.value.is_some() && update.ranges.is_none())
            .then(|| self.placed_now(rewrite))
            .flatten();
        let flushed = update.flushed.unwrap_or_else(super::echoes::clock);
        taken.own = update.value.as_deref().is_some_and(|value| {
            !(update.stale && value != current) && self.echoes.is_own(value, update.value_revision)
        });
        let echo = update.value.as_deref().map(|value| {
            if update.stale && value != current {
                return Echo::Skip;
            }
            match update.value_revision {
                Some(revision) => self.echoes.received_at(value, revision, current, flushed),
                None => self.echoes.received(value, current, flushed),
            }
        });
        taken.echo = echo;
        taken.own &= echo == Some(Echo::Apply);
        self.confirm(rewrite);
        // Only a resend is a new copy of the document: the revision sent
        // last may be a text this update just replaced (a reset's report
        // overtaken by a text of the host's own), which names that text.
        if echo == Some(Echo::Resend) {
            self.note_resent(current);
        }
        let value = update.value.as_deref();
        let shown = echo == Some(Echo::Apply) && value.is_some_and(|value| value != current);
        let mut own = None;
        if shown && let Some(value) = value {
            let revision = update.value_revision.unwrap_or_else(issue);
            self.log.own(Arc::from(value), revision);
            self.log.printed_as(revision, Print::of(value));
            own = Some(revision);
            taken.shown = true;
        }
        // Where a list read in the value is: the text of the host's own,
        // the host's text when the value is it, the version the echo
        // engine matched, else the value is a text from outside.
        let value_anchor = |link: &mut Self| -> Anchor {
            if let Some(revision) = own {
                return Anchor::Kept(revision);
            }
            let value = value.unwrap_or_default();
            if value == current {
                return Anchor::Kept(link.log.host_revision());
            }
            if let Some(revision) = update.value_revision
                && link.log.named(revision) == Named::Kept
            {
                return Anchor::Kept(revision);
            }
            match link.log.printed(Print::of(value)) {
                Some(revision) => Anchor::Kept(revision),
                None => Anchor::Outside(Arc::from(value)),
            }
        };
        let resolve = |link: &mut Self, anchor: Anchor, text: Option<Arc<str>>| match anchor {
            Anchor::Value => value_anchor(link),
            Anchor::Kept(revision) if link.log.named(revision) != Named::Kept => {
                text.map_or(Anchor::Kept(revision), Anchor::Outside)
            }
            anchor => anchor,
        };
        if let Some(anchor) = ranges_anchor
            && let Some(listed) = update.ranges
        {
            let anchor = resolve(self, anchor, ranges_text);
            if anchor != Anchor::Again {
                self.give(anchor, &listed.items);
                self.ranges_listed = Some((listed, update.ranges_for.clone()));
                self.carried_list = None;
                self.after_reset = false;
                taken.ranges = true;
            }
        } else if shown && update.ranges.is_none() {
            self.carry(before, own.unwrap_or_default());
            taken.ranges = true;
        }
        if let Some(anchor) = diagnostics_anchor
            && let Some(listed) = update.diagnostics
        {
            let anchor = resolve(self, anchor, diagnostics_text);
            if anchor != Anchor::Again {
                let placed = self.diagnostics_in_document(anchor, &listed.items, rewrite);
                self.diagnostics.clone_from(&placed);
                taken.diagnostics = Some(placed);
                let named = update
                    .diagnostics_for
                    .clone()
                    .or_else(|| update.ranges_for.clone());
                self.diagnostics_listed = Some((listed, named));
            }
        } else if shown {
            // A new text: the diagnostics given for the one it replaced are
            // given again only with it.
            self.diagnostics_listed = None;
        }
        Ok(taken)
    }

    /// The host gave `value` of its own (`replaceValue`), the host's text
    /// being `current`: what it holds, no echo to wait for. The given
    /// ranges are carried into it. Returns whether it replaced the
    /// document.
    pub(crate) fn replaced(&mut self, value: &str, current: &str, rewrite: Rewrite<'_>) -> bool {
        let sent = self.echoes.replaced(value);
        if value == current {
            return false;
        }
        let before = self.placed_now(rewrite);
        let revision = sent.unwrap_or_else(issue);
        self.log.own(Arc::from(value), revision);
        self.log.printed_as(revision, Print::of(value));
        let listed = self.ranges_listed.clone();
        self.carry(before, revision);
        self.ranges_listed = listed;
        self.after_reset = true;
        self.diagnostics_listed = None;
        true
    }

    /// The editor sent `text` to the host (`on_change`): its revision.
    pub(crate) fn sent(&mut self, text: &str) -> u64 {
        // The text sent before, if no host heard it, is no version now,
        // unless the ranges in force name it.
        self.log
            .forget_unheard(self.given.as_ref().map(|given| given.revision));
        let revision = self.echoes.sent(text);
        let print = self.echoes.print_sent(revision);
        self.log.sent(revision, text, print);
        revision
    }

    /// The text sent last reached no host (no `on_change`): it is no echo
    /// to wait for, and no version once the next is sent.
    pub(crate) fn untold(&mut self) {
        if let Some(revision) = self.echoes.last_sent() {
            self.echoes.untold(revision);
            self.log.unheard(revision);
        }
    }

    /// The echo engine heard texts come back: the versions of the log
    /// before the newest of them can no longer be named, and the ranges
    /// given for one of them are moved to the oldest kept (exactly: through
    /// the steps between).
    fn confirm(&mut self, rewrite: Rewrite<'_>) {
        let Some(back) = self.echoes.came_back() else {
            return;
        };
        if let Some(given) = &mut self.given
            && let Some(oldest) = self.log.kept_after(back)
            && given.revision < oldest
        {
            match self
                .log
                .map_between(given.revision, oldest, &given.ranges, rewrite)
            {
                Some(ranges) => {
                    given.ranges = ranges;
                    given.revision = oldest;
                }
                None => given.revision = oldest,
            }
        }
        self.log.confirmed(back);
    }

    /// The echo engine sent the document `text` again (the host was left on
    /// an older text): a version of the log, as every text sent is.
    pub(crate) fn note_resent(&mut self, text: &str) {
        if let Some(revision) = self.echoes.last_sent()
            && self.log.host_revision() < revision
            && self.log.named(revision) != Named::Kept
        {
            let print = self.echoes.print_sent(revision);
            self.log.sent(revision, text, print);
        }
    }

    /// The pause after an echo left the host on an older text is over:
    /// whether the host is to hear `current` again (see `Echoes::settle`).
    pub(crate) fn settle(&mut self, current: &str) -> bool {
        let again = self.echoes.settle(current);
        if again {
            self.note_resent(current);
        }
        again
    }

    /// Reads `items` (in the text `anchor` names) and keeps them as the
    /// given ranges.
    fn give(&mut self, anchor: Anchor, items: &[paths::TextRange]) {
        let given = match anchor {
            Anchor::Kept(revision) => {
                let text = self
                    .log
                    .text_at(revision)
                    .unwrap_or_else(|| self.log.document().clone());
                let ranges = bytes(&text, items);
                Given {
                    revision,
                    texts: texts_of(&text, &ranges),
                    ranges,
                }
            }
            Anchor::Outside(text) => {
                let document = self.log.document().clone();
                let carried = carry::moved_between(&text, &document, Some(items.to_vec()), None)
                    .0
                    .unwrap_or_default();
                let ranges = bytes(&document, &carried);
                let revision = self.log.mark();
                Given {
                    revision,
                    texts: texts_of(&document, &ranges),
                    ranges,
                }
            }
            Anchor::Again | Anchor::Value => return,
        };
        self.given = Some(given);
    }

    /// The given ranges, carried from where the document held them
    /// (`before`, with that text) into the host's own text at `revision`
    /// by the safe diff (whole changed lines where it cannot tell): no
    /// list came with it.
    fn carry(&mut self, before: Option<(Arc<str>, Vec<TextRange>)>, revision: u64) {
        if let Some((listed, _)) = self.ranges_listed.take() {
            self.carried_list = listed.statement;
        }
        let Some((old, ranges)) = before else {
            return;
        };
        let document = self.log.document().clone();
        let carried = carry::carried(&old, &document, &positions(&old, &ranges));
        let ranges = bytes(&document, &carried);
        self.given = Some(Given {
            revision,
            texts: texts_of(&document, &ranges),
            ranges,
        });
    }

    /// The given ranges in the log's document, with the text the document
    /// holds now: `None` without ranges.
    fn placed_now(&mut self, rewrite: Rewrite<'_>) -> Option<(Arc<str>, Vec<TextRange>)> {
        let given = self
            .given
            .as_ref()
            .filter(|given| !given.ranges.is_empty())?;
        let ranges = self
            .log
            .map(given.revision, &given.ranges, rewrite)
            .unwrap_or_else(|| given.ranges.clone());
        Some((self.log.document().clone(), ranges))
    }

    /// The given ranges in the log's document, each with the text it held
    /// in the text it was given for (a range whose text the document does
    /// not hold where it lands locks nothing and holds the save).
    pub(crate) fn placed(&self, rewrite: Rewrite<'_>) -> (Vec<TextRange>, Vec<String>) {
        let Some(given) = &self.given else {
            return (Vec::new(), Vec::new());
        };
        let ranges = self
            .log
            .map(given.revision, &given.ranges, rewrite)
            .unwrap_or_else(|| given.ranges.clone());
        (ranges, given.texts.clone())
    }

    /// The given ranges in the text they name (rebuilt from the log), with
    /// that text: `None` without ranges.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "protection reads it")
    )]
    pub(crate) fn given_at_anchor(&mut self) -> Option<(Arc<str>, Vec<TextRange>)> {
        let given = self.given.as_ref()?;
        let ranges = given.ranges.clone();
        let text = self.log.text_at(given.revision)?;
        Some((text, ranges))
    }

    /// `items` (diagnostics in the text `anchor` names) in the log's
    /// document.
    fn diagnostics_in_document(
        &mut self,
        anchor: Anchor,
        items: &[HostDiagnostic],
        rewrite: Rewrite<'_>,
    ) -> Vec<HostDiagnostic> {
        let document = self.log.document().clone();
        match anchor {
            Anchor::Kept(revision) => {
                let text = self
                    .log
                    .text_at(revision)
                    .unwrap_or_else(|| document.clone());
                if *text == *document {
                    return items.to_vec();
                }
                let lines = Lines::new(&text);
                let to = Lines::new(&document);
                let map = |range: TextRange| {
                    self.log
                        .map(revision, &[range], rewrite)
                        .and_then(|ranges| ranges.first().copied())
                        .unwrap_or(range)
                };
                items
                    .iter()
                    .cloned()
                    .map(|mut diagnostic| {
                        diagnostic.target = match diagnostic.target {
                            DiagnosticTarget::Range(range) => DiagnosticTarget::Range(
                                position_range(&to, map(byte_range(&lines, range))),
                            ),
                            DiagnosticTarget::Position(at) => {
                                let at = text_size(lines.byte(at));
                                let placed = map(TextRange::empty(at));
                                DiagnosticTarget::Position(to.position(usize::from(placed.start())))
                            }
                            target => target,
                        };
                        diagnostic
                    })
                    .collect()
            }
            Anchor::Outside(text) => {
                carry::moved_between(&text, &document, None, Some(items.to_vec()))
                    .1
                    .unwrap_or_default()
            }
            Anchor::Again | Anchor::Value => items.to_vec(),
        }
    }
}

/// `range` (lines and columns of the text `lines` index) as bytes.
fn byte_range(lines: &Lines<'_>, range: paths::TextRange) -> TextRange {
    let start = text_size(lines.byte(range.start));
    TextRange::new(start, text_size(lines.byte(range.end)).max(start))
}

/// `ranges` (lines and columns of `text`) as byte ranges.
fn bytes(text: &str, ranges: &[paths::TextRange]) -> Vec<TextRange> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let lines = Lines::new(text);
    ranges
        .iter()
        .map(|range| byte_range(&lines, *range))
        .collect()
}

/// What each of `ranges` holds in `text` (what is in the text of a range
/// past its end).
fn texts_of(text: &str, ranges: &[TextRange]) -> Vec<String> {
    ranges
        .iter()
        .map(|range| {
            let end = range.end().min(TextSize::of(text));
            slice::slice(text, TextRange::new(range.start().min(end), end))
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// `range` (bytes of the text `lines` index) as lines and columns.
fn position_range(lines: &Lines<'_>, range: TextRange) -> paths::TextRange {
    paths::TextRange::new(
        lines.position(usize::from(range.start())),
        lines.position(usize::from(range.end())),
    )
}

/// `ranges` (bytes of `text`) as lines and columns.
fn positions(text: &str, ranges: &[TextRange]) -> Vec<paths::TextRange> {
    let lines = Lines::new(text);
    ranges
        .iter()
        .map(|range| position_range(&lines, *range))
        .collect()
}

/// One document's channel to its host: the echo engine, the host log and
/// the lists as given (see the module), shared by its editor and the
/// binding that made it, and passed on to an editor that replaces it (a
/// framework re-mount), so nothing is read again in a text it was not
/// given for.
#[derive(Clone, Debug)]
pub struct HostChannel(Rc<RefCell<Link>>);

impl HostChannel {
    /// A channel for a document the host gave as `value` (at `revision`,
    /// its `value_revision`, when it gave one); `echoing`: the host hands
    /// every `on_change` text back. `handed`: the
    /// [`HostChannel::handoff`] of the editor that showed this document
    /// last, when the binding knows it shows the same document again (the
    /// element that editor was in, a document listed again with its id):
    /// the late echoes of that editor are told apart here too.
    pub fn new(echoing: bool, value: &str, revision: Option<u64>, handed: Option<Handoff>) -> Self {
        // The record works in the editor's text (LF breaks, no byte order
        // mark), whatever form the host writes.
        let value = crate::editor::line_endings::editor_text(value.to_string());
        Self(Rc::new(RefCell::new(Link::new(
            echoing, &value, revision, handed,
        ))))
    }

    /// The texts sent that the host has not handed back yet, for the
    /// editor a binding makes next for the same document (see
    /// [`HostChannel::new`]); `None` when none is on its way.
    pub fn handoff(&self) -> Option<Handoff> {
        self.with(|link| link.echoes.handoff())
    }

    /// Runs `apply` on the record.
    pub(crate) fn with<T>(&self, apply: impl FnOnce(&mut Link) -> T) -> T {
        apply(&mut self.0.borrow_mut())
    }

    /// The text `update`'s `list` names places in, to read its numbers in
    /// (see [`Link::reading`]), the host's text now being `current`.
    pub fn reading(
        &self,
        list: List,
        update: &HostUpdate,
        current: &str,
    ) -> Result<Reading, AnchorError> {
        self.with(|link| link.reading(list, update, current))
    }

    /// The revision of the text sent last, if any is on its way.
    pub fn latest_revision(&self) -> Option<u64> {
        self.with(|link| link.echoes.latest_revision())
    }

    /// The revision of the text sent or given last: what a host passes
    /// back as `value_revision` (a reset `replaceValue` made included).
    pub fn sent_revision(&self) -> Option<u64> {
        self.with(|link| {
            link.echoes
                .latest_revision()
                .or_else(|| Some(link.log.host_revision()))
        })
    }

    /// The revision of the text sent last, on its way or not.
    pub fn last_sent(&self) -> Option<u64> {
        self.with(|link| link.echoes.last_sent())
    }

    /// Takes `update` while no editor shows the document (before it mounts,
    /// a document not shown yet), the host's text now being `current`:
    /// as [`Link::take`], texts no edit made followed by the safe diff.
    pub fn take_unmounted(
        &self,
        mut update: HostUpdate,
        current: &str,
    ) -> Result<Taken, AnchorError> {
        // In the editor's line breaks, as an editor reads them.
        let lines = |text: &str| crate::editor::state::normalize_newlines(text.to_string());
        update.value = update.value.as_deref().map(lines);
        for anchor in [&mut update.ranges_for, &mut update.diagnostics_for] {
            if let Some(RangesFor::Text(text)) = anchor {
                *text = Arc::from(lines(text).as_str());
            }
        }
        self.with(|link| link.take(update, &lines(current), &diff_rewrite))
    }

    /// A new editor takes this channel over with `value` as its document
    /// (see [`Link::remounted`]).
    pub fn remount(&self, value: &str) {
        let value = crate::editor::state::normalize_newlines(value.to_string());
        self.with(|link| link.remounted(&value, &diff_rewrite));
    }

    /// The text sent last reached no host (no `onChange`): it is no echo
    /// to wait for.
    pub fn untold(&self) {
        self.with(Link::untold);
    }

    /// The statement of the ranges list whose locks a text of the host's
    /// own carried, until a list is given (see [`Link::carried_list`]).
    pub fn carried_list(&self) -> Option<u64> {
        self.with(|link| link.carried_list)
    }

    /// The document as the channel knows it: the record's own copy,
    /// shared, not copied.
    pub fn document(&self) -> Arc<str> {
        self.with(|link| link.log.document().clone())
    }

    /// An editor's edit, for a simulated one (the native tests of the
    /// bindings): the bytes `replaced` of the document replaced by `insert`.
    pub fn edited(&self, replaced: std::ops::Range<usize>, insert: &str) {
        self.with(|link| {
            let before = link.log.document().clone();
            let change = crate::editor::edit::Change::replace(
                &before,
                TextRange::new(text_size(replaced.start), text_size(replaced.end)),
                insert,
            );
            if let Some(after) = change.apply(&before) {
                let after: Arc<str> = Arc::from(after);
                link.log.edited(&change, "", &before, &after);
            }
        });
    }

    /// The editor sent the document to its host: its revision.
    pub fn sent(&self, text: &str) -> u64 {
        self.with(|link| link.sent(text))
    }

    /// The pause after an echo left the host on an older text is over:
    /// whether the host is to hear `current` again.
    pub fn settle(&self, current: &str) -> bool {
        self.with(|link| link.settle(current))
    }

    /// The given ranges as the document holds them (bytes), each with the
    /// text it held in the text it was given for.
    pub fn placed(&self) -> Vec<(std::ops::Range<usize>, String)> {
        self.with(|link| {
            let (ranges, texts) = link.placed(&diff_rewrite);
            ranges
                .into_iter()
                .zip(texts)
                .map(|(range, text)| (usize::from(range.start())..usize::from(range.end()), text))
                .collect()
        })
    }
}

/// `ranges` of `from` in `to`, a text no edit made of it, by the safe diff
/// (whole changed lines where it cannot tell): where no editor follows
/// fields.
fn diff_rewrite(from: &str, to: &str, ranges: &[TextRange]) -> Vec<TextRange> {
    let moved = carry::moved_between(from, to, Some(positions(from, ranges)), None)
        .0
        .unwrap_or_default();
    bytes(to, &moved)
}

#[cfg(test)]
#[path = "host_inputs_tests.rs"]
mod tests;
