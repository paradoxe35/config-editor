#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HistoryEntry {
    pub(super) value: String,
    pub(super) cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryGroup {
    Typing,
    Deleting,
    Other,
}

#[derive(Default)]
pub(super) struct EditHistory {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    last_group: Option<HistoryGroup>,
    last_edit_at: f64,
}

/// Idle gap, in milliseconds, that splits one undo step from the next.
/// Keystrokes arriving faster than this merge into a single Ctrl+Z unit;
/// a pause longer than this starts a fresh unit. Matches the typical
/// `performance.now()` resolution for typing bursts without lumping
/// deliberate pauses together.
pub(super) const UNDO_GROUPING_WINDOW_MS: f64 = 800.0;

impl EditHistory {
    pub(super) fn record_input(
        &mut self,
        before: String,
        cursor: usize,
        input_type: &str,
        timestamp: f64,
    ) {
        let group = match input_type {
            "insertText" | "insertCompositionText" => HistoryGroup::Typing,
            "deleteContentBackward" | "deleteContentForward" => HistoryGroup::Deleting,
            _ => HistoryGroup::Other,
        };
        let groupable = group != HistoryGroup::Other
            && self.last_group == Some(group)
            && timestamp - self.last_edit_at <= UNDO_GROUPING_WINDOW_MS;
        if !groupable {
            self.undo.push(HistoryEntry {
                value: before,
                cursor,
            });
        }
        self.redo.clear();
        self.last_group = Some(group);
        self.last_edit_at = timestamp;
    }

    pub(super) fn record_action(&mut self, before: String, cursor: usize) {
        self.undo.push(HistoryEntry {
            value: before,
            cursor,
        });
        self.redo.clear();
        self.last_group = None;
    }

    pub(super) fn undo(&mut self, current: String, cursor: usize) -> Option<HistoryEntry> {
        let previous = self.undo.pop()?;
        self.redo.push(HistoryEntry {
            value: current,
            cursor,
        });
        self.last_group = None;
        Some(previous)
    }

    pub(super) fn redo(&mut self, current: String, cursor: usize) -> Option<HistoryEntry> {
        let next = self.redo.pop()?;
        self.undo.push(HistoryEntry {
            value: current,
            cursor,
        });
        self.last_group = None;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the undo-grouping window: keystrokes within the window merge into
    /// one undo step, while a longer pause starts a fresh step.
    #[test]
    fn typing_pause_beyond_the_grouping_window_splits_undo_steps() {
        assert_eq!(UNDO_GROUPING_WINDOW_MS, 800.0);
        let mut history = EditHistory::default();
        history.record_input("".into(), 0, "insertText", 0.0);
        history.record_input("a".into(), 1, "insertText", 100.0);
        // 1000ms later: past the window, so a new undo step starts.
        history.record_input("ab".into(), 2, "insertText", 1100.0);

        // Two typing groups on the undo stack.
        assert!(history.undo("abc".into(), 3).is_some());
        assert!(history.undo("ab".into(), 2).is_some());
        assert_eq!(history.undo("a".into(), 1), None);
    }

    #[test]
    fn groups_adjacent_typing_and_keeps_actions_as_separate_undo_steps() {
        let mut history = EditHistory::default();
        history.record_input("".into(), 0, "insertText", 0.0);
        history.record_input("a".into(), 1, "insertText", 100.0);
        history.record_action("ab".into(), 2);

        assert_eq!(
            history.undo("a: ".into(), 3),
            Some(HistoryEntry {
                value: "ab".into(),
                cursor: 2,
            })
        );
        assert_eq!(
            history.undo("ab".into(), 2),
            Some(HistoryEntry {
                value: "".into(),
                cursor: 0,
            })
        );
    }

    #[test]
    fn redo_is_cleared_after_a_new_edit() {
        let mut history = EditHistory::default();
        history.record_action("old".into(), 0);
        assert_eq!(
            history.undo("new".into(), 3),
            Some(HistoryEntry {
                value: "old".into(),
                cursor: 0,
            })
        );
        history.record_action("old".into(), 0);
        assert_eq!(history.redo("newer".into(), 5), None);
    }
}
