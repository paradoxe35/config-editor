//! Undo and redo. Each step keeps the document and selection before it and
//! the selection after it, so both directions restore exactly where the user
//! was. Typing (or deleting) continues a step only where the previous one
//! ended and within [`UNDO_GROUPING_WINDOW_MS`]; a caret move, a paste, or a
//! command starts a new one.

/// A selection as `(start, end)` UTF-16 offsets; equal for a caret.
pub(super) type Selection = (usize, usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) value: String,
    pub(super) selection: Selection,
}

#[derive(Clone, Debug)]
struct Step {
    before: Snapshot,
    after: Selection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Typing,
    Deleting,
}

/// Steps kept for undo; the oldest are dropped beyond this.
const MAX_STEPS: usize = 500;

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
            "insertText" | "insertCompositionText" => Some(Group::Typing),
            "deleteContentBackward" | "deleteContentForward" => Some(Group::Deleting),
            _ => None,
        };
        let continues = group.is_some()
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
            self.push(Step { before, after });
        }
        self.open = group.map(|group| (group, timestamp));
    }

    /// An edit the editor made (a command, an accepted suggestion): always
    /// its own step.
    pub(super) fn record_action(&mut self, before: Snapshot, after: Selection) {
        self.push(Step { before, after });
        self.open = None;
    }

    fn push(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > MAX_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// The state before the newest step, given the current text.
    pub(super) fn undo(&mut self, current: String) -> Option<Snapshot> {
        let step = self.undo.pop()?;
        self.redo.push(Step {
            before: Snapshot {
                value: current,
                selection: step.after,
            },
            after: step.before.selection,
        });
        self.open = None;
        Some(step.before)
    }

    /// The state after the newest undone step, given the current text.
    pub(super) fn redo(&mut self, current: String) -> Option<Snapshot> {
        let step = self.redo.pop()?;
        self.undo.push(Step {
            before: Snapshot {
                value: current,
                selection: step.after,
            },
            after: step.before.selection,
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
        };
        history.record_action(before.clone(), (0, 13));
        assert_eq!(history.undo("  a: 1\n  b: 2".into()), Some(before));
        assert_eq!(
            history.redo("a: 1\nb: 2".into()),
            Some(Snapshot {
                value: "  a: 1\n  b: 2".into(),
                selection: (0, 13),
            })
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
}
