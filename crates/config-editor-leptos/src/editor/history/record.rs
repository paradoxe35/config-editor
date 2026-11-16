//! What undoing (or redoing) a step does to the text: the change the edits
//! that made the step made, inverted (as Helix keeps a step), composed over
//! the edits a step grew by. An undo or redo then hands the locks and
//! placeholders that exact change (know, don't guess), not two texts to
//! diff.

use super::super::edit::{Change, Edit};

/// The change popping a step makes of the text it is popped from.
#[derive(Clone, Debug)]
pub(in crate::editor) struct Record {
    change: Change,
    /// Whether every edit of the step said its change (see
    /// `Edit::change`); else one was read off the texts, and an undo of it
    /// is followed as such an edit is.
    told: bool,
}

impl Default for Record {
    fn default() -> Self {
        Record {
            change: Change::unchanged(""),
            told: false,
        }
    }
}

impl Record {
    /// The record of a step `edit` made: its change, inverted (popping the
    /// step gives back the text it was made on); `None` for a change not
    /// made on the edit's text.
    pub(in crate::editor) fn undoing(edit: &Edit) -> Option<Self> {
        let made = edit.made().clone();
        let made = made.clone().narrowed(&edit.before).unwrap_or(made);
        Some(Record {
            change: made.invert(&edit.before)?,
            told: edit.told(),
        })
    }

    /// This step's record (popping it from the text after it) after a later
    /// edit grew the step, `later` popping that edit: undoing the two in
    /// turn.
    pub(super) fn after(self, later: Record) -> Option<Self> {
        Some(Record {
            change: later.change.compose(&self.change)?,
            told: self.told && later.told,
        })
    }

    /// The record of the step popping this one makes the way back: this
    /// change undone, made on `from` (the text this one is popped from).
    pub(super) fn reversed(&self, from: &str) -> Option<Self> {
        Some(Record {
            change: self.change.invert(from)?,
            told: self.told,
        })
    }

    /// The record made on the text the host's change `host` gives (both
    /// made on the text the step is popped from), and the host's change as
    /// made on the text the step gives back; `None` when the two touch (see
    /// `rebase`).
    pub(super) fn carried(&self, host: &Change) -> Option<(Self, Change)> {
        let (change, host) = super::rebase::carried(&self.change, host)?;
        Some((
            Record {
                change,
                told: self.told,
            },
            host,
        ))
    }

    /// Whether popping the step would change nothing of `text` (the text
    /// it is popped from).
    pub(super) fn changes_nothing(&self, text: &str) -> bool {
        self.change.is_identity(text)
    }

    /// Bytes it holds, for the history's budget.
    pub(super) fn held_bytes(&self) -> usize {
        self.change.held_bytes()
    }

    /// The change.
    pub(super) fn change(&self) -> &Change {
        &self.change
    }

    /// Whether every edit of the step said its change.
    pub(super) fn told(&self) -> bool {
        self.told
    }

    /// The record of a step that turned `before` into `after`, read off
    /// the two texts.
    #[cfg(test)]
    pub(super) fn of_texts(before: &str, after: &str) -> Self {
        Record {
            change: Change::diffed(after, before),
            told: true,
        }
    }
}
