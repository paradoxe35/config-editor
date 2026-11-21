//! The host log: how each text the host knows became the document, so a
//! list of positions the host worked out on one of them (its read-only
//! ranges, its diagnostics) is mapped to the document through the edits
//! that made it, never guessed from the two texts.
//!
//! A **version** is a text the host knows: one sent with `on_change` (by
//! its revision), or a text of the host's own the document took (it gets a
//! revision from the same page-wide clock, kept inside). The newest version
//! is the host's text now; the document may be newer (edits the host has
//! not heard: a save held, or a typing pause while editing as another
//! language). A version keeps no text: the text of an older one is rebuilt
//! from the document by undoing the steps since ([`HostLog::text_at`]).
//!
//! A **step** is what made the next version (or the document) of one:
//! the edit pipeline's own record of each edit ([`Change`]), composed edit
//! by edit with what it replaced (as an undo step keeps it), or, for a
//! text no edit made (a conversion back from another language, a switch),
//! the two texts, followed by fields ([`Step::Rewritten`]).
//!
//! A drag is a move: the browser sends its delete and its drop as two
//! edits, which compose into a delete and an insert, so a place inside the
//! dragged text would map to nowhere. The open step keeps the stretch a
//! drag's delete took, and the drop that writes that text whole records it
//! as moved there.
//!
//! What is kept is bounded: at most [`VERSIONS_KEPT`] versions, and when
//! the host hands a revision back only the versions before it go
//! ([`HostLog::confirmed`]: a host never goes back to an older text, but a
//! server check that ran on one may come back late and name it); a text
//! of the host's own starts a new log (the texts sent before belong to the
//! text it replaced). A host that hands nothing back (no `echoes`) also
//! names at most [`VERSIONS_KEPT`] versions.

use std::collections::VecDeque;
use std::sync::Arc;

use super::super::edit::{Assoc, Change, Moved};
use super::super::text::{TextRange, TextSize, slice};
use super::echoes::Print;

/// How many versions a log keeps: older ones are out of date. A host that
/// hands texts back drops only those before the one it handed back.
pub(crate) const VERSIONS_KEPT: usize = 64;

/// How a position list is followed across a text no edit made (an Edit-as
/// conversion): from, to, ranges.
pub(crate) type Rewrite<'a> = &'a dyn Fn(&str, &str, &[TextRange]) -> Vec<TextRange>;

/// What a revision is to a log (see [`HostLog::named`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    /// A version the log keeps: positions in it map to the document.
    Kept,
    /// A revision issued (the page's editors share one clock) that this
    /// log does not keep: the host handed a newer one back or gave a text
    /// of its own since, or another editor sent it (one a hide and show
    /// replaced, say).
    OutOfDate,
    /// Never issued: no editor on the page sent it.
    Unknown,
}

/// A text the host knows.
#[derive(Clone, Copy, Debug)]
struct Version {
    revision: u64,
    /// Its print, when known (the echo engine's, for a text sent with
    /// `echoes`): a text given again is found by it.
    print: Option<Print>,
    /// A text the host knows (sent, or its own); else a mark the editor
    /// made to name the document (see [`HostLog::mark`]).
    told: bool,
}

/// What made one text of another.
#[derive(Clone, Debug)]
enum Step {
    /// Edits, exactly: `change` makes the text after of the text before,
    /// `undo` the text before of the text after.
    Exact { change: Change, undo: Change },
    /// A text no edit made (a conversion, a switch): positions are followed
    /// from `from` to `to` by fields.
    Rewritten { from: Arc<str>, to: Arc<str> },
}

/// One document's host log (see the module).
#[derive(Clone, Debug)]
pub(crate) struct HostLog {
    /// The newest text: the document as the log knows it.
    document: Arc<str>,
    /// The versions kept, oldest first; never empty.
    versions: VecDeque<Version>,
    /// The steps after each version: `steps[i]` make `versions[i + 1]` of
    /// `versions[i]`, the last ones the document of the newest version.
    steps: VecDeque<Vec<Step>>,
    /// The stretch of the open step's first text the last edit, a drag's
    /// delete, took, and its text: the drop that comes next writes it
    /// elsewhere.
    dragged: Option<(TextRange, String)>,
    /// The first revision this log issued.
    first: u64,
    /// Whether a host hands texts back (`echoes`); without, at most
    /// [`VERSIONS_KEPT`] versions are kept.
    confirms: bool,
    /// The text of the version asked for last (see [`HostLog::text_at`]).
    asked: Option<(u64, Arc<str>)>,
}

impl HostLog {
    /// A log on `document`, a text of the host's own at `revision`.
    pub(crate) fn new(document: Arc<str>, revision: u64, confirms: bool) -> Self {
        Self {
            document,
            versions: VecDeque::from([Version {
                revision,
                print: None,
                told: true,
            }]),
            steps: VecDeque::from([Vec::new()]),
            dragged: None,
            first: revision,
            confirms,
            asked: None,
        }
    }

    /// Whether the host hands texts back (`echoes`).
    pub(crate) fn set_confirms(&mut self, confirms: bool) {
        self.confirms = confirms;
        self.bound();
    }

    /// The document as the log knows it.
    pub(crate) fn document(&self) -> &Arc<str> {
        &self.document
    }

    /// The revision of the host's text now: the newest version.
    pub(crate) fn host_revision(&self) -> u64 {
        self.versions
            .iter()
            .rev()
            .find(|version| version.told)
            .or(self.versions.front())
            .map_or(self.first, |version| version.revision)
    }

    /// Notes that version `revision` is `print` (a text of the host's own,
    /// read once as it came).
    pub(crate) fn printed_as(&mut self, revision: u64, print: Print) {
        if let Some(index) = self.index(revision) {
            self.versions[index].print = Some(print);
        }
    }

    /// A version for the document as it is now, to name it (the ranges a
    /// diff moved into it): the newest version when the document is that
    /// text, else a mark at a new revision.
    pub(crate) fn mark(&mut self) -> u64 {
        if self.steps.back().is_some_and(Vec::is_empty)
            && let Some(newest) = self.versions.back()
        {
            return newest.revision;
        }
        let revision = super::echoes::issue();
        self.versions.push_back(Version {
            revision,
            print: None,
            told: false,
        });
        self.steps.push_back(Vec::new());
        self.dragged = None;
        revision
    }

    /// The revisions kept, oldest first.
    #[cfg(test)]
    pub(crate) fn revisions(&self) -> Vec<u64> {
        self.versions
            .iter()
            .map(|version| version.revision)
            .collect()
    }

    /// The host gave `document`, a text of its own, at `revision`: a new
    /// log (the texts sent before belong to the text it replaced; naming
    /// one of them is out of date).
    pub(crate) fn own(&mut self, document: Arc<str>, revision: u64) {
        self.document = document;
        self.versions = VecDeque::from([Version {
            revision,
            print: None,
            told: true,
        }]);
        self.steps = VecDeque::from([Vec::new()]);
        self.dragged = None;
        self.asked = None;
    }

    /// An edit of input `kind` (its `inputType`, empty for another edit)
    /// made `after` the document of `before` with `change`. Each edit is
    /// narrowed to what it changed on its own text (a whole text pasted
    /// with one change is that change) and composed onto the open step as
    /// it is: two edits that happen to write back a text the other took
    /// stay two (a character deleted and the same one typed elsewhere is
    /// another character).
    pub(crate) fn edited(
        &mut self,
        change: &Change,
        kind: &str,
        before: &Arc<str>,
        after: &Arc<str>,
    ) {
        self.moved_to(before);
        let Some((change, undo)) = change
            .clone()
            .narrowed(before)
            .and_then(|change| Some((change.invert(before)?, change)))
            .map(|(undo, change)| (change, undo))
        else {
            // Not a change of `before`: no record reaches past it.
            self.rewritten(after);
            return;
        };
        let steps = self.last_steps();
        // The open step's edits, taken out to compose onto (put back when
        // the edit cannot be composed).
        let earlier = match steps.pop() {
            Some(Step::Exact { change, undo }) => Some((change, undo)),
            Some(other) => {
                steps.push(other);
                None
            }
            None => None,
        };
        let composed = match &earlier {
            None => Some((change.clone(), undo.clone())),
            Some((record, record_undo)) => record
                .compose(&change)
                .map(|composed| carried(composed, record, &change))
                .and_then(|composed| Some((composed, undo.compose(record_undo)?))),
        };
        let Some((mut composed, composed_undo)) = composed else {
            if let Some((change, undo)) = earlier {
                self.last_steps().push(Step::Exact { change, undo });
            }
            self.rewritten(after);
            return;
        };
        let mut dragged = None;
        match kind {
            "deleteByDrag" => {
                dragged = drag_taken(earlier.as_ref().map(|(_, undo)| undo), before, &change);
            }
            "insertFromDrop" => {
                if let Some((stretch, text)) = self.dragged.as_ref() {
                    composed = dropped(composed, &change, *stretch, text);
                }
            }
            _ => {}
        }
        self.last_steps().push(Step::Exact {
            change: composed,
            undo: composed_undo,
        });
        self.dragged = dragged;
        self.document = after.clone();
        self.asked = None;
    }

    /// The document became `document` with no recorded edit (a conversion
    /// back from another language, a switch): positions are followed into
    /// it by fields.
    pub(crate) fn moved_to(&mut self, document: &Arc<str>) {
        if Arc::ptr_eq(&self.document, document) || *self.document == **document {
            self.document = document.clone();
            return;
        }
        self.rewritten(document);
    }

    /// The steps after the newest version.
    fn last_steps(&mut self) -> &mut Vec<Step> {
        if self.steps.is_empty() {
            self.steps.push_back(Vec::new());
        }
        self.steps
            .back_mut()
            .expect("a step list was just made when there was none")
    }

    fn rewritten(&mut self, document: &Arc<str>) {
        let from = self.document.clone();
        self.last_steps().push(Step::Rewritten {
            from,
            to: document.clone(),
        });
        self.dragged = None;
        self.document = document.clone();
        self.asked = None;
    }

    /// `text` was sent to the host at `revision` (printed `print`, when the
    /// echo engine read it): the document is that version now.
    pub(crate) fn sent(&mut self, revision: u64, text: &str, print: Option<Print>) {
        if *self.document != *text {
            self.rewritten(&Arc::from(text));
        }
        self.versions.push_back(Version {
            revision,
            print,
            told: true,
        });
        self.steps.push_back(Vec::new());
        self.dragged = None;
        self.bound();
    }

    /// Keeps at most [`VERSIONS_KEPT`] versions for a host that hands
    /// nothing back.
    fn bound(&mut self) {
        if self.confirms {
            return;
        }
        while self.versions.len() > VERSIONS_KEPT {
            self.versions.pop_front();
            self.steps.pop_front();
        }
    }

    /// The host handed `revision` back: the versions before it may go,
    /// oldest first, while the log holds more than [`VERSIONS_KEPT`] (until
    /// then a late check of an older text still names one).
    pub(crate) fn confirmed(&mut self, revision: u64) {
        while self.versions.len() > VERSIONS_KEPT
            && self
                .versions
                .front()
                .is_some_and(|version| version.revision < revision)
            && self
                .versions
                .get(1)
                .is_some_and(|version| version.revision <= revision)
        {
            self.versions.pop_front();
            self.steps.pop_front();
        }
    }

    /// The oldest revision the log keeps once the host handed `revision`
    /// back (see [`HostLog::confirmed`]).
    pub(crate) fn kept_after(&self, revision: u64) -> Option<u64> {
        let dropped = self
            .versions
            .iter()
            .zip(self.versions.iter().skip(1))
            .take(self.versions.len().saturating_sub(VERSIONS_KEPT))
            .take_while(|(version, next)| version.revision < revision && next.revision <= revision)
            .count();
        self.versions.get(dropped).map(|version| version.revision)
    }

    /// What `revision` is to this log.
    pub(crate) fn named(&self, revision: u64) -> Named {
        if self.index(revision).is_some() {
            Named::Kept
        } else if revision < super::echoes::clock() {
            Named::OutOfDate
        } else {
            Named::Unknown
        }
    }

    fn index(&self, revision: u64) -> Option<usize> {
        self.versions
            .binary_search_by_key(&revision, |version| version.revision)
            .ok()
    }

    /// The revision of the one version kept printed `print`, if exactly one
    /// is (a text given again with no revision).
    pub(crate) fn printed(&self, print: Print) -> Option<u64> {
        let mut found = self
            .versions
            .iter()
            .filter(|version| version.print == Some(print));
        let first = found.next()?;
        found.next().is_none().then_some(first.revision)
    }

    /// The text of version `revision`, rebuilt from the document by undoing
    /// the steps since; `None` when it is not kept. The last one asked for
    /// is kept.
    pub(crate) fn text_at(&mut self, revision: u64) -> Option<Arc<str>> {
        if let Some((asked, text)) = &self.asked
            && *asked == revision
        {
            return Some(text.clone());
        }
        let index = self.index(revision)?;
        let mut text = self.document.clone();
        'steps: for steps in self.steps.iter().skip(index).rev() {
            for step in steps.iter().rev() {
                match step {
                    Step::Exact { undo, .. } => {
                        text = Arc::from(undo.apply(&text)?);
                    }
                    Step::Rewritten { from, .. } => {
                        text = from.clone();
                        // Nothing before it is read: the steps before
                        // this one end at `from`.
                        continue 'steps;
                    }
                }
            }
        }
        self.asked = Some((revision, text.clone()));
        Some(text)
    }

    /// `ranges` of version `revision`, in the document: through each step
    /// since by position (a range an edit wrote inside lands where its
    /// ends went), and through a text no edit made by `rewrite` (from,
    /// to, ranges). `None` when the version is not kept.
    pub(crate) fn map(
        &self,
        revision: u64,
        ranges: &[TextRange],
        rewrite: Rewrite<'_>,
    ) -> Option<Vec<TextRange>> {
        let index = self.index(revision)?;
        Some(self.mapped_through(index, self.steps.len(), ranges, rewrite))
    }

    /// `ranges` of version `from`, in version `to` (a later one).
    pub(crate) fn map_between(
        &self,
        from: u64,
        to: u64,
        ranges: &[TextRange],
        rewrite: Rewrite<'_>,
    ) -> Option<Vec<TextRange>> {
        let (from, to) = (self.index(from)?, self.index(to)?);
        (from <= to).then(|| self.mapped_through(from, to, ranges, rewrite))
    }

    /// `ranges` through the steps after versions `from..to`.
    fn mapped_through(
        &self,
        from: usize,
        to: usize,
        ranges: &[TextRange],
        rewrite: Rewrite<'_>,
    ) -> Vec<TextRange> {
        let mut ranges = ranges.to_vec();
        for steps in self.steps.iter().skip(from).take(to - from) {
            for step in steps {
                match step {
                    Step::Exact { change, .. } => {
                        for range in &mut ranges {
                            *range = mapped(change, range);
                        }
                    }
                    Step::Rewritten { from, to } => ranges = rewrite(from, to, &ranges),
                }
            }
        }
        ranges
    }

    /// The change from version `revision` to the document, composed from
    /// the edits since; `None` when it is not kept, or a text no edit made
    /// lies between.
    pub(crate) fn change_since(&self, revision: u64) -> Option<Change> {
        let index = self.index(revision)?;
        let mut since: Option<Change> = None;
        for steps in self.steps.iter().skip(index) {
            for step in steps {
                let Step::Exact { change, .. } = step else {
                    return None;
                };
                since = Some(match since {
                    None => change.clone(),
                    Some(since) => since.compose(change)?,
                });
            }
        }
        Some(since.unwrap_or_else(|| Change::unchanged(&self.document)))
    }

    /// The steps kept, for the count test: one per gap of exact edits.
    #[cfg(test)]
    pub(crate) fn step_count(&self) -> usize {
        self.steps.iter().map(Vec::len).sum()
    }
}

/// `range` through `change`: whole when no edit wrote inside it, else
/// where its ends went.
fn mapped(change: &Change, range: &TextRange) -> TextRange {
    change.map_range(range).unwrap_or_else(|| {
        let start = change.map_pos(range.start(), Assoc::After).pos;
        let end = change.map_pos(range.end(), Assoc::Before).pos;
        TextRange::new(start, end.max(start))
    })
}

/// `composed` (`record` then `change`) with the stretches `record` moved
/// that `change` left whole, where `change` put them: composing keeps only
/// the replacements.
fn carried(composed: Change, record: &Change, change: &Change) -> Change {
    if record.moved().is_empty() {
        return composed;
    }
    let moved: Vec<Moved> = record
        .moved()
        .iter()
        .filter_map(|moved| {
            let went = change.map_range(&TextRange::at(moved.to, moved.len))?;
            (went.len() == moved.len).then_some(Moved {
                to: went.start(),
                ..*moved
            })
        })
        .collect();
    composed.with_moved(moved)
}

/// The stretch of the open step's first text a drag's delete `change`
/// (made on `before`) took, and its text: `None` unless it took one
/// stretch that the edits before it (undone by `earlier`) left whole.
fn drag_taken(
    earlier: Option<&Change>,
    before: &str,
    change: &Change,
) -> Option<(TextRange, String)> {
    let mut edits = change.edits();
    let (taken, inserted) = edits.next()?;
    if edits.next().is_some() || !inserted.is_empty() || taken.is_empty() {
        return None;
    }
    let text = slice::slice(before, taken)?.to_string();
    let stretch = match earlier {
        None => taken,
        // Left whole by the edits before (no edit wrote inside it), it is
        // the same text there.
        Some(undo) => undo.map_range(&taken)?,
    };
    (stretch.len() == taken.len()).then_some((stretch, text))
}

/// `composed` (from the open step's first text) with the drop `change`
/// added, which writes `text`, the stretch `stretch` its drag took, whole
/// at one place: that stretch recorded as moved there.
fn dropped(composed: Change, change: &Change, stretch: TextRange, text: &str) -> Change {
    let mut edits = change.edits();
    let Some((at, inserted)) = edits.next() else {
        return composed;
    };
    if edits.next().is_some() || !at.is_empty() || inserted != text {
        return composed;
    }
    // Where the drop's text starts in the text after: the drop inserts at
    // `at` of the text before it, nothing before that place changed.
    let moved = Moved {
        from: stretch.start(),
        len: TextSize::of(text),
        to: at.start(),
    };
    let kept = composed.moved().to_vec();
    composed.with_moved(kept.into_iter().chain([moved]))
}

#[cfg(test)]
#[path = "host_log_tests.rs"]
mod tests;
