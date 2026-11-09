//! [`EditorState::commit`]: the stages, run in order (see the module docs
//! of `edit` for why each sits where it does).

use leptos::prelude::*;

use super::super::fold_ranges::FoldRange;
use super::super::history::{Selection, Snapshot};
use super::super::languages::Language;
use super::super::positioning::set_caret_visible;
use super::super::state::{EditorState, textarea_selection};
use super::super::sync::map_through_change;
use super::{After, Edit, Origin};

/// The stages, in the order an edit passes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum Stage {
    Prepare,
    Check,
    /// The check refused the edit: it ends here, having changed nothing.
    Refused,
    Apply,
    Record,
    Notify,
}

/// Why an edit was refused: read-only (no message), or the reason a
/// read-only field gives.
type Refusal = Option<String>;

/// The textarea's selection and scroll position before a host value.
type KeptView = (Selection, i32, i32);

impl EditorState {
    /// Makes `edit`, through every stage. Returns whether it was made; a
    /// refused edit changes nothing (text, selection, folds, history,
    /// dirty flag) and notifies no one.
    pub(in crate::editor) fn commit(&self, mut edit: Edit) -> bool {
        let changed = edit.before != edit.text;
        // A switch, or an undo or redo back across one: another language's
        // text, whole (see `Origin::Switch`).
        let language_before = self.sync().edit_language.get_untracked();
        let switching = edit
            .language
            .filter(|language| *language != language_before);
        let origin = if switching.is_some() {
            Origin::Switch
        } else {
            edit.origin.clone()
        };

        trace(Stage::Prepare);
        let opened = self.take_opened_folds();
        if changed && origin.widens() {
            self.widen_edit(&mut edit);
        }
        // Another language's lines: each fold is carried by its field.
        let folded_fields = match switching {
            Some(_) => {
                let fields = self.folded_fields(&edit.before, language_before);
                self.forget_folds();
                fields
            }
            None => Vec::new(),
        };

        trace(Stage::Check);
        if changed && origin.is_checked() {
            let refusal = if self.config().read_only.get_untracked() {
                Some(None)
            } else {
                self.check_edit(&edit).err().map(Some)
            };
            if let Some(reason) = refusal {
                trace(Stage::Refused);
                self.refuse(&edit, opened, reason);
                return false;
            }
        }

        trace(Stage::Apply);
        if let Some(language) = switching {
            self.show_language(language, &edit);
        }
        if changed && origin.follows() {
            self.follow_edit(&edit.before, &edit.text, edit.caret());
        }
        let kept = match edit.after {
            After::Mapped => self.textarea().map(|area| {
                let selection = textarea_selection(&area);
                (selection, area.scroll_top(), area.scroll_left())
            }),
            _ => None,
        };
        if changed && !edit.origin.is_checked() {
            self.protect().trust_text();
        }
        let rewritten = self.write_text(edit.text.clone());
        let rewritten = switching
            .is_some_and(|language| self.fold_fields(&folded_fields, language))
            || rewritten;
        if origin.redetects_indent() {
            self.redetect_indent();
        }
        self.find_protected_again(&origin);
        let after = self.place_selection(&edit, rewritten, kept);

        trace(Stage::Record);
        self.record(&edit, changed, after, language_before);
        if origin.is_checked() {
            self.note_applied(&edit);
        } else {
            self.forget_refusal();
        }

        trace(Stage::Notify);
        if edit.origin.notifies() {
            self.notify_change();
        }
        if let Some(language) = switching {
            self.report_language(language);
        }
        true
    }

    /// Writes `text` as the document, the only place that does: closed
    /// folds move with the lines, and the textarea shows the result.
    /// Returns whether the textarea's text had to be rewritten (its
    /// selection is then lost).
    fn write_text(&self, text: String) -> bool {
        let moved = self.folds_through(&text);
        self.doc()
            .text
            .with_untracked(|before| self.note_text_change(before, &text));
        self.doc().text.set(text);
        self.settle_folds(moved)
    }

    /// Ends a refused edit: the document never changed, so the textarea
    /// shows it again, folds as they were, with the selection the edit
    /// started from; a read-only field's reason shows at the caret.
    fn refuse(&self, edit: &Edit, opened: Option<Vec<FoldRange>>, reason: Refusal) {
        if let Some(folds) = opened {
            self.restore_folds(folds);
        }
        self.show_text();
        let (start, end) = edit.replaced(edit.caret());
        if let Some(area) = self.textarea() {
            let _ = area.restore_selection(start as u32, end as u32);
        }
        self.note_caret(start);
        if let Some(reason) = reason {
            self.protect().flash(reason.clone(), start);
            // Typing refused part-way: nothing half-written stays.
            if let Some(take_back) = self.note_refused(edit, reason) {
                self.commit(take_back);
            }
        }
    }

    /// Puts the selection where the edit leaves it, and records the caret.
    /// Returns the selection left, for the undo step.
    fn place_selection(&self, edit: &Edit, rewritten: bool, kept: Option<KeptView>) -> Selection {
        let area = self.textarea();
        match edit.after {
            After::Select {
                anchor,
                caret,
                reveal,
            } => {
                self.note_caret(caret);
                let (start, end) = (anchor.min(caret), anchor.max(caret));
                if let Some(area) = area {
                    if reveal {
                        self.doc().text.with_untracked(|text| {
                            set_caret_visible(&area, text, caret, self.view().scroll_y)
                        });
                    }
                    // The text between the anchor and the caret stays
                    // selected.
                    let _ = area.set_selection_range(start as u32, end as u32);
                }
                (start, end)
            }
            After::Typed { selection, forced } => {
                // A rewritten textarea lost the selection the browser left.
                let after = match area {
                    Some(area) => {
                        if rewritten || forced {
                            let _ =
                                area.set_selection_range(selection.0 as u32, selection.1 as u32);
                        }
                        textarea_selection(&area)
                    }
                    None => selection,
                };
                self.note_caret(after.0);
                after
            }
            After::Mapped => {
                // The caret stays on the same text and the view where it
                // was: the caret is mapped through the change here, so the
                // later `prop:value` (identical text) leaves the selection
                // alone, and the scroll position writing the value moved
                // is put back.
                let (Some(area), Some(((start, end), top, left))) = (area, kept) else {
                    return (0, 0);
                };
                let map = |offset: usize| map_through_change(&edit.before, &edit.text, offset);
                let (start, end) = (map(start), map(end));
                let _ = area.set_selection_range(start as u32, end as u32);
                area.set_scroll_top(top);
                area.set_scroll_left(left);
                self.view().scroll_x.set(f64::from(area.scroll_left()));
                self.view().scroll_y.set(f64::from(area.scroll_top()));
                (start, end)
            }
            After::Kept => (0, 0),
        }
    }

    /// The undo history, by origin; `after` is the selection the edit left.
    fn record(&self, edit: &Edit, changed: bool, after: Selection, language: Language) {
        let before = || Snapshot {
            value: edit.before.clone(),
            selection: edit.replaced(after.0),
            language: None,
        };
        let history = self.doc().history;
        match &edit.origin {
            Origin::Command | Origin::Reload { .. } => {
                history.update(|history| history.record_action(before(), after));
            }
            Origin::Input { kind, time } => {
                if changed {
                    history.update(|history| history.record_input(before(), after, kind, *time));
                }
            }
            Origin::Undo => history.update(|history| {
                history.undo(edit.before.clone());
            }),
            Origin::Redo => history.update(|history| {
                history.redo(edit.before.clone());
            }),
            Origin::Switch => match edit.language {
                // Its own step, undone into the language it left.
                Some(next) if next != language => history.update(|history| {
                    let before = Snapshot {
                        language: Some(language),
                        ..before()
                    };
                    history.record_switch(before, after, next);
                }),
                _ => history.update(|history| history.record_action(before(), after)),
            },
            // Not a step: the steps before it take the host's change.
            Origin::HostValue => history.update(|history| history.rebase(&edit.before, &edit.text)),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The stages edits passed on this thread, for the tests.
    static TRACE: std::cell::RefCell<Vec<Stage>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Notes that an edit reached `stage` (for the tests; nothing otherwise).
fn trace(_stage: Stage) {
    #[cfg(test)]
    TRACE.with(|trace| trace.borrow_mut().push(_stage));
}

/// The stages edits passed on this thread since the last call.
#[cfg(all(test, feature = "yaml"))]
pub(in crate::editor) fn take_trace() -> Vec<Stage> {
    TRACE.with(|trace| std::mem::take(&mut *trace.borrow_mut()))
}
