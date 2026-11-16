//! Undo and redo. Each step keeps the document and selection before it and
//! the selection after it, so both directions restore exactly where the user
//! was. Typing (or deleting) continues a step only where the previous one
//! ended and within [`UNDO_GROUPING_WINDOW_MS`]; a caret move, a paste, or a
//! command starts a new one. An edit that changes no text is no step (and
//! ends none), so every step undone changes the text. An Edit-as switch is a step too: its snapshots
//! carry the language their text is in, so undo goes back to the view it
//! left and redo returns to the new one. A step that said which folds it
//! leaves closed (a line command moving closed folds) keeps the folds of
//! both sides, so undo and redo close them again.

use super::bridge::links::Links;
use super::fold_ranges::FoldRange;
use super::languages::Language;
use std::sync::Arc;

mod rebase;
mod record;

pub(super) use record::Record;

/// A selection as `(start, end)` UTF-16 offsets; equal for a caret.
pub(super) type Selection = (usize, usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) value: String,
    pub(super) selection: Selection,
    /// The language `value` is in, for a step that changed the language on
    /// screen (an Edit-as switch); `None` for every other step.
    pub(super) language: Option<Language>,
    /// The folds closed in `value`, for a step that said them (a line
    /// command); `None` for every other step: the folds follow the change.
    pub(super) folds: Option<Vec<FoldRange>>,
    /// What an Edit-as view keeps with `value`: undo and redo put it back
    /// with the text.
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
}

impl ViewState {
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
        };
        self.links.bytes(newer.map(|newer| &newer.links)) + saved
    }
}

#[derive(Clone, Debug)]
struct Step {
    before: Snapshot,
    after: Selection,
    /// The language the step left, when it changed it.
    after_language: Option<Language>,
    /// The folds the step left closed, when it said them.
    after_folds: Option<Vec<FoldRange>>,
    /// What popping the step does to the text it is popped from.
    record: Record,
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

/// Bytes of earlier texts kept for undo (each step keeps the whole text
/// before it): past this the oldest steps are dropped, the newest always
/// kept, so a large document keeps fewer steps (500 steps of a 50,000-line
/// text would hold about a gigabyte).
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

impl EditHistory {
    /// An edit the browser made (typing, deleting, pasting), from the input
    /// event's type, leaving `text`. A step that grew back to the text it
    /// started from (`-` typed, then the Enter that ends a list takes it
    /// away) is no step any more: undo never changes nothing.
    #[cfg(test)]
    pub(super) fn record_input(
        &mut self,
        before: Snapshot,
        text: &str,
        after: Selection,
        input_type: &str,
        timestamp: f64,
    ) {
        self.record_input_as(
            before,
            text,
            after,
            (input_type, timestamp),
            Record::default(),
        );
    }

    /// [`Self::record_input`] of an edit popped as `record` says.
    pub(super) fn record_input_as(
        &mut self,
        before: Snapshot,
        text: &str,
        after: Selection,
        (input_type, timestamp): (&str, f64),
        record: Record,
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
        if dropping && let Some(step) = self.undo.last_mut() {
            step.after = after;
            step.record = std::mem::take(&mut step.record).after(record);
            self.open = None;
            if step.before.value == text {
                self.undo.pop();
            }
            return;
        }
        // Typing over a wrapped selection: the wrap and the typing are one.
        let replaces_wrapped = group == Some(Group::Typing)
            && self.open.is_some_and(|(open, _)| open == Group::Wrapping)
            && self
                .undo
                .last()
                .is_some_and(|step| step.after == before.selection);
        let continues = composing
            || replaces_wrapped
            || group.is_some()
                && self.open.is_some_and(|(open, at)| {
                    Some(open) == group && timestamp - at <= UNDO_GROUPING_WINDOW_MS
                })
                && self
                    .undo
                    .last()
                    .is_some_and(|step| step.after == before.selection);
        if continues && let Some(step) = self.undo.last_mut() {
            if step.before.value == text {
                self.undo.pop();
                self.open = None;
                return;
            }
            step.after = after;
            step.record = std::mem::take(&mut step.record).after(record);
        } else {
            self.push(Step {
                before,
                after,
                after_language: None,
                after_folds: None,
                record,
            });
        }
        self.open = group.map(|group| (group, timestamp));
    }

    /// The caret stepped from `from` over a closer the editor wrote to `to`
    /// (a `"` typed before the `"` it put there) at `timestamp`: typing
    /// still open goes on past it in the same step, so one undo takes back
    /// a member typed in one go, its pairs with it.
    pub(super) fn stepped_over(&mut self, from: usize, to: usize, timestamp: f64) {
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

    /// An edit the editor made (a command, an accepted suggestion): always
    /// its own step.
    pub(super) fn record_action(&mut self, before: Snapshot, after: Selection) {
        self.record_command(before, after, None, Record::default());
    }

    /// [`Self::record_action`] for a command leaving `after_folds` closed
    /// when it says so (`before` then holds the folds closed before it).
    pub(super) fn record_command(
        &mut self,
        before: Snapshot,
        after: Selection,
        after_folds: Option<Vec<FoldRange>>,
        record: Record,
    ) {
        self.push(Step {
            before,
            after,
            after_language: None,
            after_folds,
            record,
        });
        self.open = None;
    }

    /// An Edit-as switch from `before` (its language in it) to `language`:
    /// its own step.
    pub(super) fn record_switch(&mut self, before: Snapshot, after: Selection, language: Language) {
        self.push(Step {
            before,
            after,
            after_language: Some(language),
            after_folds: None,
            record: Record::default(),
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
                bytes += step.before.value.len() + step.before.view.bytes(newer);
                newer = Some(&step.before.view);
                bytes <= MAX_BYTES
            })
            .count()
            .max(1);
        let dropped = self.undo.len() - kept;
        self.undo.drain(..dropped);
    }

    /// Forgets every step (a host text shown in another language: the
    /// steps' texts are in the one it left).
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The host replaced the text `current` with `new` (a new controlled
    /// `value`): the change is no step of its own, and the steps before it
    /// take it too, so undo takes back only the user's edits and keeps the
    /// host's (see [`rebase`]). The newest step whose edit overlaps the
    /// host's change goes with every older one; redo starts over.
    pub(super) fn rebase(&mut self, current: &str, new: &str) {
        let change = rebase::HostChange::new(current, new);
        self.redo.clear();
        self.open = None;
        // From the newest step back: each step's text, and the text after
        // it, with the change made.
        let (mut after_old, mut after_new) = (current.to_string(), new.to_string());
        let mut kept = self.undo.len();
        for index in (0..self.undo.len()).rev() {
            let step = &mut self.undo[index];
            let rebased = match step.before.language.or(step.after_language) {
                Some(_) => None,
                None => change.applied(&step.before.value),
            };
            let Some(rebased) = rebased else {
                kept = self.undo.len() - index - 1;
                break;
            };
            let map = |offset: usize| rebase::mapped(&after_old, &after_new, offset);
            step.after = (map(step.after.0), map(step.after.1));
            let before = std::mem::replace(&mut step.before.value, rebased.clone());
            // The host's change moved the lines the folds were said on, and
            // the items the links are on.
            (step.before.folds, step.after_folds) = (None, None);
            // The text is no longer one saved, nor the change recorded.
            step.before.view = ViewState::default();
            step.record = Record::default();
            let (start, end) = step.before.selection;
            step.before.selection = (
                rebase::mapped(&before, &rebased, start),
                rebase::mapped(&before, &rebased, end),
            );
            (after_old, after_new) = (before, rebased);
        }
        let dropped = self.undo.len() - kept;
        self.undo.drain(..dropped);
    }

    /// The state before the newest step, given the current text.
    #[cfg(test)]
    pub(super) fn undo(&mut self, current: String) -> Option<Snapshot> {
        self.undo_with(current, ViewState::default())
    }

    /// [`Self::undo`] from `current`, whose view keeps `view`.
    pub(super) fn undo_with(&mut self, current: String, view: ViewState) -> Option<Snapshot> {
        let step = self.undo.pop()?;
        self.redo.push(Step {
            record: step.record.reversed(&current),
            before: Snapshot {
                value: current,
                selection: step.after,
                language: step.after_language,
                folds: step.after_folds,
                view,
            },
            after: step.before.selection,
            after_language: step.before.language,
            after_folds: step.before.folds.clone(),
        });
        self.open = None;
        Some(step.before)
    }

    /// The host text a step's text `text` was saved as, newest first: an
    /// edit giving back a text saved before (an item pasted back where it
    /// was cut) saves as that text was saved, the host's text as it was.
    /// Only steps with a saved text, and of its length, are compared.
    pub(super) fn saved_as(&self, text: &str) -> Option<Arc<str>> {
        self.undo
            .iter()
            .chain(&self.redo)
            .rev()
            .map(|step| &step.before)
            .filter(|before| before.view.saved_as.is_some() && before.value.len() == text.len())
            .find(|before| before.value == text)
            .and_then(|before| before.view.saved_as.clone())
    }

    /// The state the newest step returns to, without moving it (see
    /// [`Self::undo`]).
    pub(super) fn next_undo(&self) -> Option<&Snapshot> {
        self.undo.last().map(|step| &step.before)
    }

    /// The state the newest undone step returns to, without moving it (see
    /// [`Self::redo`]).
    pub(super) fn next_redo(&self) -> Option<&Snapshot> {
        self.redo.last().map(|step| &step.before)
    }

    /// What the next undo (or redo, with `redo`) does to the text, when
    /// its step recorded it.
    #[cfg(feature = "protect")]
    pub(super) fn next_change(&self, redo: bool) -> Option<&super::edit::Change> {
        let steps = if redo { &self.redo } else { &self.undo };
        steps.last()?.record.change()
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

    /// The state after the newest undone step, given the current text.
    #[cfg(test)]
    pub(super) fn redo(&mut self, current: String) -> Option<Snapshot> {
        self.redo_with(current, ViewState::default())
    }

    /// [`Self::redo`] from `current`, whose view keeps `view`.
    pub(super) fn redo_with(&mut self, current: String, view: ViewState) -> Option<Snapshot> {
        let step = self.redo.pop()?;
        self.undo.push(Step {
            record: step.record.reversed(&current),
            before: Snapshot {
                value: current,
                selection: step.after,
                language: step.after_language,
                folds: step.after_folds,
                view,
            },
            after: step.before.selection,
            after_language: step.before.language,
            after_folds: step.before.folds.clone(),
        });
        self.open = None;
        Some(step.before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str, caret: usize) -> Snapshot {
        Snapshot {
            value: value.into(),
            selection: (caret, caret),
            language: None,
            folds: None,
            view: ViewState::default(),
        }
    }

    #[test]
    fn typing_pause_beyond_the_grouping_window_splits_undo_steps() {
        assert_eq!(UNDO_GROUPING_WINDOW_MS, 800.0);
        let mut history = EditHistory::default();
        history.record_input(at("", 0), "a", (1, 1), "insertText", 0.0);
        history.record_input(at("a", 1), "ab", (2, 2), "insertText", 100.0);
        history.record_input(at("ab", 2), "abc", (3, 3), "insertText", 1100.0);

        assert_eq!(history.undo("abc".into()), Some(at("ab", 2)));
        assert_eq!(history.undo("ab".into()), Some(at("", 0)));
        assert_eq!(history.undo("".into()), None);
    }

    #[test]
    fn typing_somewhere_else_is_a_separate_step() {
        let mut history = EditHistory::default();
        history.record_input(at("ab", 1), "axb", (2, 2), "insertText", 0.0);
        // The caret moved to the end before the next keystroke.
        history.record_input(at("axb", 3), "axby", (4, 4), "insertText", 100.0);
        assert_eq!(history.undo("axby".into()), Some(at("axb", 3)));
        assert_eq!(history.undo("axb".into()), Some(at("ab", 1)));
    }

    #[test]
    fn an_ime_composition_is_one_step() {
        let mut history = EditHistory::default();
        history.record_input(
            at("name: ", 6),
            "name: に",
            (7, 7),
            "insertCompositionText",
            0.0,
        );
        // Each update replaces the composed text (selected by the browser).
        let composing = |value: &str, from: usize, to: usize| Snapshot {
            value: value.into(),
            selection: (from, to),
            language: None,
            folds: None,
            view: ViewState::default(),
        };
        history.record_input(
            composing("name: に", 6, 7),
            "name: にほ",
            (8, 8),
            "insertCompositionText",
            2000.0,
        );
        history.record_input(
            composing("name: にほ", 6, 8),
            "name: にほん",
            (9, 9),
            "insertCompositionText",
            4000.0,
        );
        history.record_input(
            composing("name: にほん", 6, 9),
            "name: 日本",
            (8, 8),
            "insertCompositionText",
            6000.0,
        );
        assert_eq!(history.undo("name: 日本".into()), Some(at("name: ", 6)));
    }

    #[test]
    fn a_drag_move_is_one_step() {
        let mut history = EditHistory::default();
        history.record_input(
            at("alpha: one\nb: 2", 7),
            "alpha: \nb: 2",
            (7, 7),
            "deleteByDrag",
            0.0,
        );
        history.record_input(
            at("alpha: \nb: 2", 13),
            "alpha: \nb: 2one",
            (16, 16),
            "insertFromDrop",
            5.0,
        );
        assert_eq!(
            history.undo("alpha: \nb: 2one".into()),
            Some(at("alpha: one\nb: 2", 7))
        );
    }

    #[test]
    fn sealing_splits_consecutive_compositions() {
        let mut history = EditHistory::default();
        history.record_input(at("", 0), "日", (1, 1), "insertCompositionText", 0.0);
        history.seal();
        history.record_input(at("日", 1), "日本", (2, 2), "insertCompositionText", 10.0);
        assert_eq!(history.undo("日本".into()), Some(at("日", 1)));
    }

    #[test]
    fn typing_on_past_a_closer_stepped_over_is_one_step() {
        let mut history = EditHistory::default();
        // `"` writes `""`, `k` goes in it, `"` steps over the closer, `:`.
        history.record_input(at("{}", 1), "{\"\"}", (2, 2), "insertText", 0.0);
        history.record_input(at("{\"\"}", 2), "{\"k\"}", (3, 3), "insertText", 100.0);
        history.stepped_over(3, 4, 200.0);
        history.record_input(at("{\"k\"}", 4), "{\"k\":}", (5, 5), "insertText", 300.0);
        // A line break Enter typed goes on with it.
        history.record_input(
            at("{\"k\":}", 5),
            "{\"k\":\n}",
            (6, 6),
            "insertLineBreak",
            400.0,
        );
        assert_eq!(history.undo("{\"k\":\n}".into()), Some(at("{}", 1)));
        // Past a pause, or once the caret moved, stepping over starts nothing.
        let mut history = EditHistory::default();
        history.record_input(at("{}", 1), "{\"\"}", (3, 3), "insertText", 0.0);
        history.stepped_over(3, 4, 2000.0);
        history.record_input(at("{\"\"}", 4), "{\"\":}", (5, 5), "insertText", 2100.0);
        assert_eq!(history.undo("{\"\":}".into()), Some(at("{\"\"}", 4)));
        let mut history = EditHistory::default();
        history.record_input(at("{}", 1), "{\"\"}", (3, 3), "insertText", 0.0);
        history.stepped_over(1, 2, 100.0);
        history.record_input(at("{\"\"}", 2), "{\"\":}", (3, 3), "insertText", 200.0);
        assert_eq!(history.undo("{\"\":}".into()), Some(at("{\"\"}", 2)));
    }

    #[test]
    fn a_step_typed_back_to_its_start_is_no_step() {
        let mut history = EditHistory::default();
        history.record_action(at("", 0), (1, 1));
        // `-` typed, then the Enter ending the list takes it away.
        history.record_input(at("\n", 0), "-\n", (1, 1), "insertText", 0.0);
        history.record_input(at("-\n", 1), "\n", (0, 0), "insertLineBreak", 100.0);
        assert_eq!(history.undo("\n".into()), Some(at("", 0)));
        // A drag dropped where it started.
        let mut history = EditHistory::default();
        history.record_input(at("ab", 0), "b", (0, 0), "deleteByDrag", 0.0);
        history.record_input(at("b", 0), "ab", (1, 1), "insertFromDrop", 5.0);
        assert_eq!(history.undo("ab".into()), None);
    }

    #[test]
    fn a_paste_is_its_own_step() {
        let mut history = EditHistory::default();
        history.record_input(at("", 0), "hello", (5, 5), "insertFromPaste", 0.0);
        history.record_input(at("hello", 5), "hello!", (6, 6), "insertText", 10.0);
        assert_eq!(history.undo("hello!".into()), Some(at("hello", 5)));
    }

    #[test]
    fn undo_restores_the_selection_and_redo_the_result() {
        let mut history = EditHistory::default();
        let before = Snapshot {
            value: "a: 1\nb: 2".into(),
            selection: (0, 9),
            language: None,
            folds: None,
            view: ViewState::default(),
        };
        history.record_action(before.clone(), (0, 13));
        assert_eq!(history.undo("  a: 1\n  b: 2".into()), Some(before));
        assert_eq!(
            history.redo("a: 1\nb: 2".into()),
            Some(Snapshot {
                value: "  a: 1\n  b: 2".into(),
                selection: (0, 13),
                language: None,
                folds: None,
                view: ViewState::default(),
            })
        );
    }

    #[cfg(all(feature = "yaml", feature = "json"))]
    #[test]
    fn a_switch_is_a_step_undone_into_the_language_it_left() {
        use super::Language;
        let mut history = EditHistory::default();
        let yaml = Snapshot {
            value: "a: 1\n".into(),
            selection: (0, 0),
            language: Some(Language::YAML),
            folds: None,
            view: ViewState::default(),
        };
        history.record_switch(yaml.clone(), (4, 4), Language::JSON);
        assert_eq!(history.undo("{\"a\": 1}\n".into()), Some(yaml));
        let redone = history.redo("a: 1\n".into()).expect("the switch again");
        assert_eq!(redone.language, Some(Language::JSON));
        assert_eq!(redone.value, "{\"a\": 1}\n");
        assert_eq!(
            history
                .undo("{\"a\": 1}\n".into())
                .and_then(|step| step.language),
            Some(Language::YAML)
        );
    }

    #[test]
    fn actions_are_separate_steps_and_clear_redo() {
        let mut history = EditHistory::default();
        history.record_input(at("", 0), "a", (1, 1), "insertText", 0.0);
        history.record_action(at("a", 1), (4, 4));
        assert_eq!(history.undo("a: ".into()), Some(at("a", 1)));
        history.record_action(at("a", 1), (2, 2));
        assert_eq!(history.redo("ab".into()), None);
    }

    /// The steps kept: by count, and by the bytes of the texts they keep.
    #[test]
    fn history_is_capped_by_steps_and_by_size_keeping_the_newest() {
        let mut history = EditHistory::default();
        for step in 0..MAX_STEPS + 20 {
            history.record_action(at(&step.to_string(), 0), (0, 0));
        }
        assert_eq!(history.undo.len(), MAX_STEPS);
        assert_eq!(
            history.next_undo().map(|step| step.value.as_str()),
            Some("519")
        );

        // Steps of a large text: as many as fit in the budget, the newest.
        let mut history = EditHistory::default();
        let large = "x".repeat(MAX_BYTES / 10);
        for step in 0..30 {
            history.record_action(at(&format!("{step}{large}"), 0), (0, 0));
        }
        let kept: usize = history
            .undo
            .iter()
            .map(|step| step.before.value.len())
            .sum();
        assert!(kept <= MAX_BYTES, "{kept} bytes kept");
        assert_eq!(history.undo.len(), 9);
        assert!(
            history
                .next_undo()
                .is_some_and(|step| step.value.starts_with("29x"))
        );

        // One step larger than the budget is still undoable.
        let mut history = EditHistory::default();
        let huge = "y".repeat(MAX_BYTES + 1);
        history.record_action(at(&huge, 0), (0, 0));
        assert_eq!(history.undo.len(), 1);
    }

    /// An undo and a redo of a step typed in two edits make exactly the
    /// change the step recorded, undone and made again: of two alike
    /// lines, the one typed into is the one changed back (a diff of the
    /// texts would take the first).
    #[cfg(feature = "protect")]
    #[test]
    fn undo_and_redo_make_the_change_their_step_recorded() {
        use super::super::edit::Edit;
        let typed = |before: &str, at: usize, text: &str| {
            let after = format!("{}{text}{}", &before[..at], &before[at..]);
            let caret = at + text.len();
            Edit::input(
                before.into(),
                (at, at),
                after,
                (caret, caret),
                "insertText".into(),
                0.0,
            )
        };
        let mut history = EditHistory::default();
        // `a\na\n`: an `a` typed after the second line's `a`, twice.
        let first = typed("a\na\n", 3, "a");
        let second = typed("a\naa\n", 4, "a");
        for (edit, time) in [(&first, 0.0), (&second, 100.0)] {
            let caret = edit.caret();
            history.record_input_as(
                at(&edit.before, caret - 1),
                &edit.text,
                (caret, caret),
                ("insertText", time),
                Record::undoing(edit),
            );
        }
        assert_eq!(history.undo.len(), 1);
        let undo = history
            .next_change(false)
            .expect("the undo's change")
            .clone();
        assert_eq!(undo.apply("a\naaa\n").as_deref(), Some("a\na\n"));
        assert_eq!(undo.edits().collect::<Vec<_>>(), vec![(3..5, "")]);
        history.undo("a\naaa\n".into());
        let redo = history
            .next_change(true)
            .expect("the redo's change")
            .clone();
        assert_eq!(redo.edits().collect::<Vec<_>>(), vec![(3..3, "aa")]);
        // A step the host's change rebased has no record: read off the texts.
        history.redo("a\na\n".into());
        history.rebase("a\naaa\n", "b\na\naaa\n");
        assert!(history.next_change(false).is_none());
    }
}
