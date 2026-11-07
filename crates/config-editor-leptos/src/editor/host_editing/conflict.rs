//! A new baseline from the host: taken silently when nothing is unsaved,
//! otherwise held back while a notice asks the user to reload it or keep
//! their text.
//!
//! The notice never takes the focus: it opens while the user may be typing,
//! and a key in flight would land on its buttons (Space or Enter choosing
//! for them). It is `role="alert"`, so it is announced, and `F6` moves the
//! focus to it and back.

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::state::{EditorState, textarea_selection};
use super::super::sync::{map_through_change, remember_synced};
use super::{BaselineConflict, BaselineDecision, PendingBaseline, decide_baseline};

impl EditorState {
    /// Handles `new` (normalized) arriving as the host's baseline.
    pub(in crate::editor) fn offer_baseline(&self, new: String) {
        let editing = self.editing;
        let pending = editing.conflict.get_untracked();
        if pending.as_ref().is_some_and(|pending| pending.host == new) {
            return;
        }
        let in_use = editing.baseline.get_untracked();
        let decision = if new == in_use {
            BaselineDecision::Unchanged
        } else {
            decide_baseline(
                &in_use,
                &new,
                editing.dirty.get_untracked(),
                self.is_document(&new),
                editing.sent.with_value(|sent| sent.contains(&new)),
            )
        };
        match decision {
            BaselineDecision::Unchanged | BaselineDecision::Take => {
                if decision == BaselineDecision::Take {
                    editing.baseline.set(new);
                }
                if pending.is_some() {
                    self.close_conflict(BaselineConflict::Settled);
                }
            }
            BaselineDecision::Conflict => {
                let shown = self.shown_quietly(&new);
                editing
                    .conflict
                    .set(Some(PendingBaseline { host: new, shown }));
                if pending.is_none() {
                    self.report_conflict(BaselineConflict::Opened);
                }
            }
        }
    }

    /// Reload: the text becomes the new baseline, as one edit (one undo step,
    /// sent through `on_change`, converted back while editing as another
    /// language, to the host's text exactly).
    pub(in crate::editor) fn reload_baseline(&self) {
        let Some(pending) = self.editing.conflict.get_untracked() else {
            return;
        };
        super::super::input::flush_input(*self);
        self.editing.baseline.set(pending.host.clone());
        let before = self.doc.text.get_untracked();
        if before != pending.shown {
            if self.bridge().is_some() {
                remember_synced(*self, pending.shown.clone(), pending.host.clone());
            }
            let element = self.textarea();
            let selection = element.as_ref().map_or_else(
                || {
                    let at = self.doc.caret.get_untracked();
                    (at, at)
                },
                textarea_selection,
            );
            let caret = map_through_change(&before, &pending.shown, selection.0);
            self.close_suggestions();
            self.hide_hover();
            self.forget_typing_aids();
            // The host's own text: read-only fields do not hold it back, and
            // its secret placeholders are the host's.
            self.commit(
                Edit::new(before, selection, pending.shown, caret)
                    .quiet()
                    .reloading(pending.host.clone()),
            );
        }
        self.close_conflict(BaselineConflict::Reloaded);
    }

    /// Keep mine: the text stays and is compared with the new baseline.
    pub(in crate::editor) fn keep_mine(&self) {
        let Some(pending) = self.editing.conflict.get_untracked() else {
            return;
        };
        self.editing.baseline.set(pending.host);
        self.close_conflict(BaselineConflict::KeptMine);
    }

    fn close_conflict(&self, how: BaselineConflict) {
        let focused_notice = self.notice_has_focus();
        self.editing.conflict.set(None);
        self.report_conflict(how);
        if focused_notice && let Some(element) = self.textarea() {
            // Back to the text where it was, without scrolling to the caret.
            let (top, left) = (element.scroll_top(), element.scroll_left());
            let _ = element.focus();
            element.set_scroll_top(top);
            element.set_scroll_left(left);
        }
    }

    fn report_conflict(&self, how: BaselineConflict) {
        if let Some(callback) = self.editing.on_conflict {
            callback.run(how);
        }
    }

    /// Whether `host` (in the host's language) is the document now.
    fn is_document(&self, host: &str) -> bool {
        let shown = self.shown_quietly(host);
        let text = self.doc.text.get_untracked();
        text == shown
            || (self.bridge().is_some()
                && text == self.sync.synced_edit.get_value()
                && self.sync.host_text.get_value() == host)
    }

    /// `host` as shown on screen, as is when it cannot convert, without
    /// reporting problems.
    fn shown_quietly(&self, host: &str) -> String {
        match self.bridge() {
            Some(bridge) => bridge
                .to_edit(host)
                .map_or_else(|_| host.to_string(), |conversion| conversion.text),
            None => host.to_string(),
        }
    }

    /// `F6` while the notice is open: from the text to Keep mine (the
    /// choice that loses nothing), and from the notice back to the text.
    pub(in crate::editor) fn notice_shortcut(&self, event: &web_sys::KeyboardEvent) -> bool {
        let plain = !(event.ctrl_key() || event.meta_key() || event.alt_key() || event.shift_key());
        if event.key() != "F6" || !plain || self.editing.conflict.with_untracked(Option::is_none) {
            return false;
        }
        event.prevent_default();
        if self.notice_has_focus() {
            if let Some(element) = self.textarea() {
                let _ = element.focus();
            }
        } else if let Some(button) = self.editing.keep_mine_ref.get_untracked() {
            let _ = button.focus();
        }
        true
    }

    fn notice_has_focus(&self) -> bool {
        let Some(active) = active_element() else {
            return false;
        };
        self.editing
            .notice_ref
            .get_untracked()
            .is_some_and(|notice| notice.contains(Some(&active)))
    }
}

fn active_element() -> Option<web_sys::Element> {
    web_sys::window()?.document()?.active_element()
}

/// While the notice is open, a text that comes to equal the new baseline
/// (the user typed the same change, or the host set the value too) closes it.
pub(super) fn settle_when_text_matches(state: EditorState) {
    Effect::new(move |_| {
        let matches = state.editing.conflict.with(|pending| {
            pending
                .as_ref()
                .is_some_and(|pending| state.doc.text.with(|text| *text == pending.shown))
        });
        if matches {
            untrack(|| {
                if let Some(pending) = state.editing.conflict.get_untracked() {
                    state.editing.baseline.set(pending.host);
                    state.close_conflict(BaselineConflict::Settled);
                }
            });
        }
    });
}
