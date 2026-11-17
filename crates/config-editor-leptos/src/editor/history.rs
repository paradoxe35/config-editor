//! Undo and redo, as Helix keeps them: each step holds the change that
//! pops it (the edit's own change, inverted) and the selection on each
//! side, never a copy of the text. Typing (or deleting) continues a step
//! only where the previous one ended and within [`UNDO_GROUPING_WINDOW_MS`],
//! its change composed onto the step's; a caret move, a paste, or a command
//! starts a new one. An edit that changes no text is no step (and ends
//! none), so every step undone changes the text. An Edit-as switch is a
//! step too: its sides carry the language their text is in, so undo goes
//! back to the view it left and redo returns to the new one. A step that
//! said which folds it leaves closed (a line command moving closed folds)
//! keeps the folds of both sides, so undo and redo close them again.
//!
//! Selections are kept as (anchor, caret) byte offsets of their side's
//! text, so a backward selection comes back backward and the host's
//! change (see [`rebase`]) maps them as it maps the steps.

#![deny(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use super::bridge::links::Links;
use super::edit::Change;
use super::fold_ranges::FoldRange;
use super::languages::Language;
use crate::editor::text::TextSize;
use std::sync::Arc;

mod rebase;
mod record;

pub(super) use record::Record;

/// A selection as `(start, end)` byte offsets of its text; equal for a
/// caret. Where a direction matters (`Edit::restore`, the history), it is
/// `(anchor, caret)`.
pub(super) type Selection = (TextSize, TextSize);

/// One side of a step, besides its text: the selection there (anchor,
/// caret, bytes of that side's text), and what else the step puts back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) selection: Selection,
    /// The language the side's text is in, for a step that changed the
    /// language on screen (an Edit-as switch); `None` for every other step.
    pub(super) language: Option<Language>,
    /// The folds closed on that side, for a step that said them (a line
    /// command); `None` for every other step: the folds follow the change.
    pub(super) folds: Option<Vec<FoldRange>>,
    /// What an Edit-as view keeps with the side's text: undo and redo put
    /// it back with the text.
    pub(super) view: ViewState,
}

/// What an undo step keeps of an Edit-as view's text besides the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ViewState {
    /// The links of the text's items to the host's (see `bridge::links`).
    pub(super) links: Links,
    /// The host text the text was saved as, when it was: an undo back to it
    /// sends that host text exactly, comments a fresh save could not bring
    /// back included.
    pub(super) saved_as: Option<Arc<str>>,
    /// The text saved as `saved_as`: an edit giving it back saves as it was
    /// saved (see [`EditHistory::saved_as`]).
    saved: Option<Arc<str>>,
}

impl ViewState {
    /// What the view keeps of `text`: its `links`, and the host text it
    /// was saved as, when it was.
    pub(super) fn of(text: &str, links: Links, saved_as: Option<Arc<str>>) -> Self {
        let saved = saved_as.as_ref().map(|_| Arc::from(text));
        Self {
            links,
            saved_as,
            saved,
        }
    }

    /// Bytes it holds, for the history's budget (see [`Links::bytes`]): a
    /// host text it shares with `newer` (the state of the step after it)
    /// counted there only.
    fn bytes(&self, newer: Option<&ViewState>) -> usize {
        let shared = matches!(
            (&self.saved_as, newer.and_then(|newer| newer.saved_as.as_ref())),
            (Some(host), Some(theirs)) if Arc::ptr_eq(host, theirs)
        );
        let saved = if shared {
            0
        } else {
            self.saved_as.as_ref().map_or(0, |host| host.len())
                + self.saved.as_ref().map_or(0, |text| text.len())
        };
        self.links.bytes(newer.map(|newer| &newer.links)) + saved
    }
}

#[derive(Clone, Debug)]
struct Step {
    before: Snapshot,
    /// The selection the step left (anchor, caret, bytes of its text).
    after: Selection,
    /// The language the step left, when it changed it.
    after_language: Option<Language>,
    /// The folds the step left closed, when it said them.
    after_folds: Option<Vec<FoldRange>>,
    /// What popping the step does to the text it is popped from.
    record: Record,
    /// The document versions the step was made between (see
    /// `DocState::version`), for the edit that made it; `None` for a step
    /// undo or redo moved.
    made_between: Option<(u64, u64)>,
}

/// What the next undo (or redo) does, without moving it.
#[derive(Clone, Debug)]
pub(super) struct Next {
    /// The change it makes of the text as it is.
    pub(super) change: Change,
    /// Whether the edits that made the step said their changes (see
    /// `Edit::change`).
    pub(super) told: bool,
    /// The side it goes back to (its selection in bytes of the text the
    /// change gives).
    pub(super) to: Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Typing,
    Deleting,
    /// An IME composition: each update replaces the previous one, so the
    /// whole composition is one step.
    Composing,
    /// Dragging selected text: the removal and the drop are one move.
    Dragging,
    /// A selection wrapped in a typed pair (`"` over `one` gives `"one"`,
    /// still selected): what is typed next replaces it in the same step,
    /// however long after.
    Wrapping,
}

/// Steps kept for undo; the oldest are dropped beyond this.
const MAX_STEPS: usize = 500;

/// Bytes the steps may hold (the text their changes write back, and the
/// views they keep): past this the oldest steps are dropped, the newest
/// always kept. A step holds what its edit replaced, so only edits
/// replacing much of a large text (a paste over all of it, a switch) count
/// for much.
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct EditHistory {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The group the newest step can still grow by, and when it last grew.
    open: Option<(Group, f64)>,
}

/// Pause, in milliseconds, after which typing starts a new undo step.
pub(super) const UNDO_GROUPING_WINDOW_MS: f64 = 800.0;

/// A step as an edit makes it: the side before it, the selection it
/// leaves, what pops it, and the document versions it was made between.
pub(super) struct Made {
    pub(super) before: Snapshot,
    pub(super) after: Selection,
    pub(super) record: Record,
    pub(super) between: (u64, u64),
}

impl EditHistory {
    /// An edit the browser made (typing, deleting, pasting) of the input
    /// event's type `input_type`, leaving `text`. A step that grew back to
    /// the text it started from (`-` typed, then the Enter that ends a list
    /// takes it away) is no step any more: undo never changes nothing.
    pub(super) fn record_input(
        &mut self,
        made: Made,
        text: &str,
        (input_type, timestamp): (&str, f64),
    ) {
        let group = match input_type {
            "insertText" | "insertLineBreak" | "insertParagraph" => Some(Group::Typing),
            "insertCompositionText" | "deleteCompositionText" | "insertFromComposition" => {
                Some(Group::Composing)
            }
            "deleteContentBackward" | "deleteContentForward" => Some(Group::Deleting),
            "deleteByDrag" => Some(Group::Dragging),
            super::keys::WRAP => Some(Group::Wrapping),
            _ => None,
        };
        let composing = group == Some(Group::Composing)
            && self.open.is_some_and(|(open, _)| open == Group::Composing);
        let dropping = input_type == "insertFromDrop"
            && self.open.is_some_and(|(open, _)| open == Group::Dragging);
        // Typing over a wrapped selection: the wrap and the typing are one.
        let replaces_wrapped = group == Some(Group::Typing)
            && self.open.is_some_and(|(open, _)| open == Group::Wrapping)
            && self
                .undo
                .last()
                .is_some_and(|step| step.after == made.before.selection);
        let continues = dropping
            || composing
            || replaces_wrapped
            || group.is_some()
                && self.open.is_some_and(|(open, at)| {
                    Some(open) == group && timestamp - at <= UNDO_GROUPING_WINDOW_MS
                })
                && self
                    .undo
                    .last()
                    .is_some_and(|step| step.after == made.before.selection);
        if continues && let Some(step) = self.undo.last_mut() {
            let Some(record) = std::mem::take(&mut step.record).after(made.record) else {
                self.clear();
                return;
            };
            if record.changes_nothing(text) {
                self.undo.pop();
                self.open = None;
                return;
            }
            step.record = record;
            step.after = made.after;
            step.made_between = step.made_between.map(|(start, _)| (start, made.between.1));
        } else {
            self.push(Step {
                before: made.before,
                after: made.after,
                after_language: None,
                after_folds: None,
                record: made.record,
                made_between: Some(made.between),
            });
        }
        self.open = if dropping {
            None
        } else {
            group.map(|group| (group, timestamp))
        };
    }

    /// The caret stepped from byte `from` over a closer the editor wrote to
    /// byte `to` (a `"` typed before the `"` it put there) at `timestamp`:
    /// typing still open goes on past it in the same step, so one undo
    /// takes back a member typed in one go, its pairs with it.
    pub(super) fn stepped_over(&mut self, from: TextSize, to: TextSize, timestamp: f64) {
        if !self.open.is_some_and(|(open, at)| {
            open == Group::Typing && timestamp - at <= UNDO_GROUPING_WINDOW_MS
        }) {
            return;
        }
        if let Some(step) = self.undo.last_mut()
            && step.after == (from, from)
        {
            step.after = (to, to);
            self.open = Some((Group::Typing, timestamp));
        }
    }

    /// Ends the open step: the next edit starts a new one (the caret moved, or
    /// a composition ended).
    pub(super) fn seal(&mut self) {
        self.open = None;
    }

    /// An edit the editor made (a command, an accepted suggestion, a
    /// reload): always its own step, leaving `after_folds` closed when it
    /// says so (`made.before` then holds the folds closed before it).
    pub(super) fn record_command(&mut self, made: Made, after_folds: Option<Vec<FoldRange>>) {
        self.push(Step {
            before: made.before,
            after: made.after,
            after_language: None,
            after_folds,
            record: made.record,
            made_between: Some(made.between),
        });
        self.open = None;
    }

    /// An Edit-as switch from `made.before` (its language in it) to
    /// `language`: its own step.
    pub(super) fn record_switch(&mut self, made: Made, language: Language) {
        self.push(Step {
            before: made.before,
            after: made.after,
            after_language: Some(language),
            after_folds: None,
            record: made.record,
            made_between: Some(made.between),
        });
        self.open = None;
    }

    fn push(&mut self, step: Step) {
        self.redo.clear();
        self.undo.push(step);
        // The newest steps that fit in both budgets.
        let mut bytes = 0;
        let mut newer: Option<&ViewState> = None;
        let kept = self
            .undo
            .iter()
            .rev()
            .take(MAX_STEPS)
            .take_while(|step| {
                bytes += step.record.held_bytes() + step.before.view.bytes(newer);
                newer = Some(&step.before.view);
                bytes <= MAX_BYTES
            })
            .count()
            .max(1);
        let dropped = self.undo.len() - kept;
        self.undo.drain(..dropped);
    }

    /// Forgets every step (a host text shown in another language: the
    /// steps' changes are of the one it left).
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The host changed the text with `host` (a new controlled `value`,
    /// read off the texts at that edge): the change is no step of its own,
    /// and the steps before it take it too, so undo takes back only the
    /// user's edits and keeps the host's (see [`rebase`]). The newest step
    /// whose edit touches the host's change goes with every older one;
    /// redo starts over.
    pub(super) fn rebase(&mut self, host: &Change) {
        self.redo.clear();
        self.open = None;
        // From the newest step back, the host's change as made on the text
        // after the step.
        let mut host = host.clone();
        let mut kept = self.undo.len();
        for index in (0..self.undo.len()).rev() {
            let step = &mut self.undo[index];
            let carried = match step.before.language.or(step.after_language) {
                Some(_) => None,
                None => step.record.carried(&host),
            };
            let Some((record, host_before)) = carried else {
                kept = self.undo.len() - index - 1;
                break;
            };
            step.after = rebase::mapped(&host, step.after);
            step.before.selection = rebase::mapped(&host_before, step.before.selection);
            // The host's change moved the lines the folds were said on, and
            // the items the links are on.
            (step.before.folds, step.after_folds) = (None, None);
            // The text is no longer one saved.
            step.before.view = ViewState::default();
            step.record = record;
            host = host_before;
        }
        let dropped = self.undo.len() - kept;
        self.undo.drain(..dropped);
    }

    /// Moves the newest step to redo: the undo it pops was made on
    /// `current`, whose view keeps `view`.
    pub(super) fn undo_with(&mut self, current: &str, view: ViewState) {
        let Some(step) = self.undo.pop() else {
            return;
        };
        let Some(record) = step.record.reversed(current) else {
            self.clear();
            return;
        };
        self.redo.push(Self::reversed(step, record, view));
        self.open = None;
    }

    /// Moves the newest undone step back to undo: the redo it pops was made
    /// on `current`, whose view keeps `view`.
    pub(super) fn redo_with(&mut self, current: &str, view: ViewState) {
        let Some(step) = self.redo.pop() else {
            return;
        };
        let Some(record) = step.record.reversed(current) else {
            self.clear();
            return;
        };
        self.undo.push(Self::reversed(step, record, view));
        self.open = None;
    }

    /// `step` the other way round, popped by `record`, its other side's
    /// view `view`.
    fn reversed(step: Step, record: Record, view: ViewState) -> Step {
        Step {
            record,
            before: Snapshot {
                selection: step.after,
                language: step.after_language,
                folds: step.after_folds,
                view,
            },
            after: step.before.selection,
            after_language: step.before.language,
            after_folds: step.before.folds,
            made_between: None,
        }
    }

    /// The host text the text `text` was saved as, newest first: an edit
    /// giving back a text saved before (an item pasted back where it was
    /// cut) saves as that text was saved, the host's text as it was. Only
    /// steps whose side was saved, and of its length, are compared.
    pub(super) fn saved_as(&self, text: &str) -> Option<Arc<str>> {
        self.undo
            .iter()
            .chain(&self.redo)
            .rev()
            .map(|step| &step.before.view)
            .find(|view| {
                view.saved
                    .as_deref()
                    .is_some_and(|saved| saved.len() == text.len() && saved == text)
            })
            .and_then(|view| view.saved_as.clone())
    }

    /// What the next undo (or redo, with `redo`) does, without moving it.
    pub(super) fn next(&self, redo: bool) -> Option<Next> {
        let steps = if redo { &self.redo } else { &self.undo };
        let step = steps.last()?;
        Some(Next {
            change: step.record.change().clone(),
            told: step.record.told(),
            to: step.before.clone(),
        })
    }

    /// The view the next undo (or redo, with `redo`) goes back to.
    pub(super) fn next_view(&self, redo: bool) -> Option<&ViewState> {
        let steps = if redo { &self.redo } else { &self.undo };
        steps.last().map(|step| &step.before.view)
    }

    /// The document versions the newest step was made between, when an
    /// edit made it (see `drag_move`: a refused drop undoes its drag's
    /// delete only when that is the newest step).
    pub(super) fn newest_made_between(&self) -> Option<(u64, u64)> {
        self.undo.last()?.made_between
    }

    /// Whether there is a step to undo.
    #[cfg(all(test, feature = "yaml"))]
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Forgets the newest undone step: an undo that took back an edit the
    /// user did not make (see `drag_move`), which redo must not make again.
    pub(super) fn forget_redo(&mut self) {
        self.redo.pop();
    }

    /// Whether an undone step can be redone.
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests;
