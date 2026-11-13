//! Undo and redo. Each step keeps the document and selection before it and
//! the selection after it, so both directions restore exactly where the user
//! was. Typing (or deleting) continues a step only where the previous one
//! ended and within [`UNDO_GROUPING_WINDOW_MS`]; a caret move, a paste, or a
//! command starts a new one. An Edit-as switch is a step too: its snapshots
//! carry the language their text is in, so undo goes back to the view it
//! left and redo returns to the new one. A step that said which folds it
//! leaves closed (a line command moving closed folds) keeps the folds of
//! both sides, so undo and redo close them again.

use super::fold_ranges::FoldRange;
use super::languages::Language;

mod rebase;

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
}

#[derive(Clone, Debug)]
struct Step {
    before: Snapshot,
    after: Selection,
    /// The language the step left, when it changed it.
    after_language: Option<Language>,
    /// The folds the step left closed, when it said them.
    after_folds: Option<Vec<FoldRange>>,
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
    /// event's type.
    pub(super) fn record_input(
        &mut self,
        before: Snapshot,
        after: Selection,
        input_type: &str,
        timestamp: f64,
    ) {
        let group = match input_type {
            "insertText" => Some(Group::Typing),
            "insertCompositionText" | "deleteCompositionText" | "insertFromComposition" => {
                Some(Group::Composing)
            }
            "deleteContentBackward" | "deleteContentForward" => Some(Group::Deleting),
            "deleteByDrag" => Some(Group::Dragging),
            _ => None,
        };
        let composing = group == Some(Group::Composing)
            && self.open.is_some_and(|(open, _)| open == Group::Composing);
        let dropping = input_type == "insertFromDrop"
            && self.open.is_some_and(|(open, _)| open == Group::Dragging);
        if dropping && let Some(step) = self.undo.last_mut() {
            step.after = after;
            self.open = None;
            return;
        }
        let continues = composing
            || group.is_some()
                && self.open.is_some_and(|(open, at)| {
                    Some(open) == group && timestamp - at <= UNDO_GROUPING_WINDOW_MS
                })
                && self
                    .undo
                    .last()
                    .is_some_and(|step| step.after == before.selection);
        if continues && let Some(step) = self.undo.last_mut() {
            step.after = after;
        } else {
            self.push(Step {
                before,
                after,
                after_language: None,
                after_folds: None,
            });
        }
        self.open = group.map(|group| (group, timestamp));
    }

    /// Ends the open step: the next edit starts a new one (the caret moved, or
    /// a composition ended).
    pub(super) fn seal(&mut self) {
        self.open = None;
    }

    /// An edit the editor made (a command, an accepted suggestion): always
    /// its own step.
    pub(super) fn record_action(&mut self, before: Snapshot, after: Selection) {
        self.record_command(before, after, None);
    }

    /// [`Self::record_action`] for a command leaving `after_folds` closed
    /// when it says so (`before` then holds the folds closed before it).
    pub(super) fn record_command(
        &mut self,
        before: Snapshot,
        after: Selection,
        after_folds: Option<Vec<FoldRange>>,
    ) {
        self.push(Step {
            before,
            after,
            after_language: None,
            after_folds,
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
        });
        self.open = None;
    }

    fn push(&mut self, step: Step) {
        self.redo.clear();
        self.undo.push(step);
        // The newest steps that fit in both budgets.
        let mut bytes = 0;
        let kept = self
            .undo
            .iter()
            .rev()
            .take(MAX_STEPS)
            .take_while(|step| {
                bytes += step.before.value.len();
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
            // The host's change moved the lines the folds were said on.
            (step.before.folds, step.after_folds) = (None, None);
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
    pub(super) fn undo(&mut self, current: String) -> Option<Snapshot> {
        let step = self.undo.pop()?;
        self.redo.push(Step {
            before: Snapshot {
                value: current,
                selection: step.after,
                language: step.after_language,
                folds: step.after_folds,
            },
            after: step.before.selection,
            after_language: step.before.language,
            after_folds: step.before.folds.clone(),
        });
        self.open = None;
        Some(step.before)
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
    pub(super) fn redo(&mut self, current: String) -> Option<Snapshot> {
        let step = self.redo.pop()?;
        self.undo.push(Step {
            before: Snapshot {
                value: current,
                selection: step.after,
                language: step.after_language,
                folds: step.after_folds,
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
        }
    }

    #[test]
    fn typing_pause_beyond_the_grouping_window_splits_undo_steps() {
        assert_eq!(UNDO_GROUPING_WINDOW_MS, 800.0);
        let mut history = EditHistory::default();
        history.record_input(at("", 0), (1, 1), "insertText", 0.0);
        history.record_input(at("a", 1), (2, 2), "insertText", 100.0);
        history.record_input(at("ab", 2), (3, 3), "insertText", 1100.0);

        assert_eq!(history.undo("abc".into()), Some(at("ab", 2)));
        assert_eq!(history.undo("ab".into()), Some(at("", 0)));
        assert_eq!(history.undo("".into()), None);
    }

    #[test]
    fn typing_somewhere_else_is_a_separate_step() {
        let mut history = EditHistory::default();
        history.record_input(at("ab", 1), (2, 2), "insertText", 0.0);
        // The caret moved to the end before the next keystroke.
        history.record_input(at("axb", 3), (4, 4), "insertText", 100.0);
        assert_eq!(history.undo("axby".into()), Some(at("axb", 3)));
        assert_eq!(history.undo("axb".into()), Some(at("ab", 1)));
    }

    #[test]
    fn an_ime_composition_is_one_step() {
        let mut history = EditHistory::default();
        history.record_input(at("name: ", 6), (7, 7), "insertCompositionText", 0.0);
        // Each update replaces the composed text (selected by the browser).
        let composing = |value: &str, from: usize, to: usize| Snapshot {
            value: value.into(),
            selection: (from, to),
            language: None,
            folds: None,
        };
        history.record_input(
            composing("name: に", 6, 7),
            (8, 8),
            "insertCompositionText",
            2000.0,
        );
        history.record_input(
            composing("name: にほ", 6, 8),
            (9, 9),
            "insertCompositionText",
            4000.0,
        );
        history.record_input(
            composing("name: にほん", 6, 9),
            (8, 8),
            "insertCompositionText",
            6000.0,
        );
        assert_eq!(history.undo("name: 日本".into()), Some(at("name: ", 6)));
    }

    #[test]
    fn a_drag_move_is_one_step() {
        let mut history = EditHistory::default();
        history.record_input(at("alpha: one\nb: 2", 7), (7, 7), "deleteByDrag", 0.0);
        history.record_input(at("alpha: \nb: 2", 13), (16, 16), "insertFromDrop", 5.0);
        assert_eq!(
            history.undo("alpha: \nb: 2one".into()),
            Some(at("alpha: one\nb: 2", 7))
        );
    }

    #[test]
    fn sealing_splits_consecutive_compositions() {
        let mut history = EditHistory::default();
        history.record_input(at("", 0), (1, 1), "insertCompositionText", 0.0);
        history.seal();
        history.record_input(at("日", 1), (2, 2), "insertCompositionText", 10.0);
        assert_eq!(history.undo("日本".into()), Some(at("日", 1)));
    }

    #[test]
    fn a_paste_is_its_own_step() {
        let mut history = EditHistory::default();
        history.record_input(at("", 0), (5, 5), "insertFromPaste", 0.0);
        history.record_input(at("hello", 5), (6, 6), "insertText", 10.0);
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
        history.record_input(at("", 0), (1, 1), "insertText", 0.0);
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
}
