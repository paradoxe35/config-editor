//! [`EditorState::commit`]: the stages, run in order (see the module docs
//! of `edit` for why each sits where it does).

use leptos::prelude::*;

use super::super::fold_ranges::FoldRange;
use super::super::history::{Made, Record, Selection, Snapshot, ViewState};
use super::super::keyboard::utf16_to_byte_index;
use super::super::languages::Language;
use super::super::positioning::set_caret_visible;
use super::super::state::{EditorState, directed_selection, select};
use super::{After, Assoc, Change, Edit, Origin};

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

/// The textarea's selection (anchor, caret) and scroll position before a
/// host value.
type KeptView = (Selection, i32, i32);

impl EditorState {
    /// Makes `edit`, through every stage. Returns whether it was made; a
    /// refused edit changes nothing (text, selection, folds, history,
    /// dirty flag) and notifies no one.
    pub(in crate::editor) fn commit(&self, mut edit: Edit) -> bool {
        let changed = edit.before != edit.text;
        // A switch, or an undo or redo back across one: another language's
        // text, whole (see `Origin::Switch`). A host value shown in the
        // host's language stays a host value.
        let language_before = self.sync().edit_language.get_untracked();
        let switching = edit
            .language
            .filter(|language| *language != language_before);
        let origin = if switching.is_some() && edit.origin != Origin::HostValue {
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
        // Test builds check that what the gate took kept every span (see
        // `protect::kept`).
        #[cfg(test)]
        let spans = (changed && origin.is_checked())
            .then(|| self.spans_before(&edit.before))
            .map(|spans| (spans, matches!(origin, Origin::Undo | Origin::Redo)));
        // The folds a command says it leaves closed, and those closed before
        // it: undo and redo close them again.
        let said_folds = (origin == Origin::Command)
            .then(|| edit.folds.clone())
            .flatten()
            .map(|after| (self.with_closed_folds_untracked(<[_]>::to_vec), after));
        // Undone or redone back into the host's language with its own text:
        // nothing new to tell it.
        let hosts_own = switching == Some(self.config().host_language)
            && self
                .sync()
                .session
                .with_value(|session| *session.host == *edit.text);
        if let Some(language) = switching {
            if language_before == self.config().host_language {
                self.leave_ranges(&edit.before);
            }
            self.show_language(language, &edit);
        }
        if changed && origin.follows() {
            self.follow_edit(&edit);
        }
        let kept = match edit.after {
            After::Mapped => self.textarea().map(|area| {
                let selection = directed_selection(&area);
                (selection, area.scroll_top(), area.scroll_left())
            }),
            _ => None,
        };
        if changed && !edit.origin.is_checked() {
            self.protect().trust_text();
        }
        // What is kept by position follows the change: folds (in
        // `write_text`), typed closers, Tab stops, the current match, the
        // problems found before it.
        let mapping = edit.mapping();
        if changed {
            self.follow_typing_aids(&mapping, &edit.before, switching.is_some());
            self.follow_current_match(&mapping);
        }
        self.follow_problems(&mapping, &edit.before);
        let version = self.doc().version.get_value();
        let rewritten = self.write_text(&mapping, edit.text.clone(), edit.folds.take());
        let links = self.note_view_edit(&edit, &origin, language_before);
        let rewritten = switching
            .is_some_and(|language| self.fold_fields(&folded_fields, language))
            || rewritten;
        if origin.redetects_indent() {
            self.redetect_indent();
        }
        self.find_protected_again(&origin);
        let after = self.place_selection(&edit, rewritten, kept);
        #[cfg(test)]
        if let Some((spans, stepped)) = spans {
            self.assert_spans_kept(&spans, &edit.before, &edit.text, edit.caret(), stepped);
        }

        trace(Stage::Record);
        let between = (version, self.doc().version.get_value());
        self.record(
            &edit,
            (changed, links),
            (after, between),
            language_before,
            said_folds,
        );

        trace(Stage::Notify);
        if edit.origin.notifies() && !hosts_own {
            self.notify_change();
        }
        if let Some(language) = switching {
            self.report_language(language);
        }
        true
    }

    /// Writes `text`, which `change` makes of the document, as the
    /// document, the only place that does: closed folds move with the lines
    /// through the change (or are `folds`, when the edit says), the
    /// document's version moves on, and the textarea shows the result.
    /// Returns whether the textarea's text had to be rewritten (its
    /// selection is then lost).
    fn write_text(&self, change: &Change, text: String, folds: Option<Vec<FoldRange>>) -> bool {
        let moved = folds.or_else(|| self.folds_through(change, &text));
        self.doc()
            .text
            .with_untracked(|before| self.note_text_change(before, &text));
        self.doc().version.update_value(|version| *version += 1);
        self.doc().text.set(text);
        self.settle_folds(moved)
    }

    /// Ends a refused edit: the document never changed, so the textarea
    /// shows it again, folds as they were, with the selection the edit
    /// started from; a read-only field's reason shows at the caret, and a
    /// read-only editor says it cannot edit (as for typing).
    fn refuse(&self, edit: &Edit, opened: Option<Vec<FoldRange>>, reason: Refusal) {
        if let Some(folds) = opened {
            self.restore_folds(folds);
        }
        self.show_text();
        let (anchor, caret) = edit.restore;
        let (start, end) = (anchor.min(caret), anchor.max(caret));
        match self.textarea() {
            Some(area) => {
                let _ = area.restore_selection(start as u32, end as u32);
                if caret < anchor {
                    select(&area, start, end, true);
                }
            }
            None => self.note_caret(caret),
        }
        let Some(reason) = reason else {
            self.note_read_only();
            return;
        };
        self.protect().flash(reason, start);
    }

    /// Puts the selection where the edit leaves it; the caret is recorded
    /// from the textarea (`SourceArea::follow_caret`), or from the edit
    /// without one. Returns the selection left as (anchor, caret), for the
    /// undo step.
    fn place_selection(&self, edit: &Edit, rewritten: bool, kept: Option<KeptView>) -> Selection {
        let area = self.textarea();
        match edit.after {
            After::Select {
                anchor,
                caret,
                reveal,
            } => {
                let (start, end) = (anchor.min(caret), anchor.max(caret));
                let Some(area) = area else {
                    self.note_caret(caret);
                    return (anchor, caret);
                };
                if reveal {
                    self.doc()
                        .text
                        .with_untracked(|text| set_caret_visible(&area, text, caret));
                }
                // The text between the anchor and the caret stays selected,
                // the caret at its own end.
                select(&area, start, end, caret < anchor);
                (anchor, caret)
            }
            After::Typed { selection, forced } => {
                // A rewritten textarea lost the selection the browser left.
                match area {
                    Some(area) => {
                        if rewritten || forced {
                            let _ =
                                area.set_selection_range(selection.0 as u32, selection.1 as u32);
                        } else {
                            area.follow_caret();
                        }
                        directed_selection(&area)
                    }
                    None => {
                        self.note_caret(selection.0);
                        selection
                    }
                }
            }
            After::Mapped => {
                // The caret stays on the same text and the view where it
                // was: the caret is mapped through the change here, so the
                // later `prop:value` (identical text) leaves the selection
                // alone, and the scroll position writing the value moved
                // is put back.
                let map = |offset: usize| edit.map_units(offset, Assoc::Before);
                let (Some(area), Some(((anchor, caret), top, left))) = (area, kept) else {
                    let caret = map(self.doc().caret.get_untracked());
                    self.note_caret(caret);
                    return (caret, caret);
                };
                let (anchor, caret) = (map(anchor), map(caret));
                let (start, end) = (anchor.min(caret), anchor.max(caret));
                select(&area, start, end, caret < anchor);
                area.set_scroll_top(top);
                area.set_scroll_left(left);
                (anchor, caret)
            }
            After::Kept => {
                // The text changed under the caret: its line and path too.
                if let Some(area) = area {
                    area.follow_caret();
                }
                (0, 0)
            }
        }
    }

    /// The undo history, by origin; `changed` says whether the edit changed
    /// the text, `after` is the selection the edit left ((anchor, caret)),
    /// `between` the document's versions before and after it, `folds` the
    /// folds closed before and after a command that said them, `view` what
    /// the view kept of the text before (see `history::ViewState`). The
    /// history keeps selections in bytes of their side's text.
    fn record(
        &self,
        edit: &Edit,
        (changed, view): (bool, ViewState),
        (after, between): (Selection, (u64, u64)),
        language: Language,
        folds: Option<(Vec<FoldRange>, Vec<FoldRange>)>,
    ) {
        let history = self.doc().history;
        let made = |language: Option<Language>, folds: Option<Vec<FoldRange>>| {
            Some(Made {
                before: Snapshot {
                    selection: bytes(&edit.before, edit.restore),
                    language,
                    folds,
                    view: view.clone(),
                },
                after: bytes(&edit.text, after),
                record: Record::undoing(edit)?,
                between,
            })
        };
        match &edit.origin {
            // An edit writing the text that is there (a replace by the
            // same text, a suggestion already written, a Tab over the
            // indent it writes) is nothing to the history: no step, and
            // typing on where it stopped goes on in the same step.
            Origin::Command | Origin::Reload { .. } | Origin::Input { .. } if !changed => {}
            Origin::Command => {
                let (closed, left) = folds.unzip();
                let made = made(None, closed);
                history.update(|history| match made {
                    Some(made) => history.record_command(made, left),
                    None => history.clear(),
                });
            }
            Origin::Reload { .. } => {
                let made = made(None, None);
                history.update(|history| match made {
                    Some(made) => history.record_command(made, None),
                    None => history.clear(),
                });
            }
            Origin::Input { kind, time } => {
                let made = made(None, None);
                history.update(|history| match made {
                    Some(made) => history.record_input(made, &edit.text, (kind, *time)),
                    None => history.clear(),
                });
            }
            Origin::Undo => {
                history.update(|history| history.undo_with(&edit.before, view.clone()));
            }
            Origin::Redo => {
                history.update(|history| history.redo_with(&edit.before, view.clone()));
            }
            Origin::Switch => {
                // Its own step, undone into the language it left.
                let next = edit.language.filter(|next| *next != language);
                let made = made(next.map(|_| language), None);
                history.update(|history| match (made, next) {
                    (Some(made), Some(next)) => history.record_switch(made, next),
                    (Some(made), None) => history.record_command(made, None),
                    (None, _) => history.clear(),
                });
            }
            // Not a step: the steps before it take the host's change; ones
            // in the language it left are another language's texts.
            Origin::HostValue => history.update(|history| match edit.language {
                Some(next) if next != language => history.clear(),
                _ => history.rebase(edit.made()),
            }),
        }
    }
}

/// `(anchor, caret)` UTF-16 offsets of `text` as bytes.
fn bytes(text: &str, (anchor, caret): Selection) -> Selection {
    (
        utf16_to_byte_index(text, anchor),
        utf16_to_byte_index(text, caret),
    )
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
