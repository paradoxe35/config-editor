//! What undoing (or redoing) a step does to the text, as the edits that
//! made the step recorded it (`Edit::change`): the change popping the step
//! makes of the text it is popped from. An undo or redo then hands the
//! locks and placeholders that exact change (know, don't guess), not two
//! texts to diff. Kept in builds with read-only fields and secrets, the
//! only readers; a step with no record (an edit that gave none, a step the
//! host's change rebased) is read off the two texts, as before.

#[cfg(feature = "protect")]
use super::super::edit::Change;
use super::super::edit::Edit;

/// The change popping a step makes, when known.
#[derive(Clone, Debug, Default)]
pub(in crate::editor) struct Record {
    #[cfg(feature = "protect")]
    change: Option<Change>,
}

impl Record {
    /// The record of a step `edit` made: its change, inverted (popping the
    /// step gives back the text it was made on).
    pub(in crate::editor) fn undoing(edit: &Edit) -> Self {
        #[cfg(feature = "protect")]
        {
            Record {
                change: edit.change().and_then(|change| change.invert(&edit.before)),
            }
        }
        #[cfg(not(feature = "protect"))]
        {
            let _ = edit;
            Record {}
        }
    }

    /// This step's record (popping it from the text after it) after a later
    /// edit grew the step, `later` popping that edit: undoing the two in
    /// turn.
    pub(super) fn after(self, later: Record) -> Self {
        #[cfg(feature = "protect")]
        {
            Record {
                change: later
                    .change
                    .zip(self.change)
                    .and_then(|(later, earlier)| later.compose(&earlier)),
            }
        }
        #[cfg(not(feature = "protect"))]
        {
            let _ = later;
            self
        }
    }

    /// The record of the step popping this one makes the way back: this
    /// change undone, made on `from` (the text this one is popped from).
    pub(super) fn reversed(&self, from: &str) -> Self {
        #[cfg(feature = "protect")]
        {
            Record {
                change: self.change.as_ref().and_then(|change| change.invert(from)),
            }
        }
        #[cfg(not(feature = "protect"))]
        {
            let _ = from;
            Record {}
        }
    }

    /// The change, when known.
    #[cfg(feature = "protect")]
    pub(super) fn change(&self) -> Option<&Change> {
        self.change.as_ref()
    }
}
