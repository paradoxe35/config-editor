//! A new baseline from the host: taken silently when nothing is unsaved,
//! otherwise held back while a notice asks the user to reload it or keep
//! their text.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::super::state::{Edit, EditorState, textarea_selection};
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
                    self.focus_notice();
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
                    .hosts_own(),
            );
            self.adopt_host_tokens(&pending.host);
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

    /// The notice takes the focus when it opens while the user is in the
    /// editor (never from elsewhere on the page), on Keep mine, the choice
    /// that loses nothing.
    fn focus_notice(&self) {
        let in_editor = self.view.root_ref.get_untracked().is_some_and(|root| {
            active_element().is_some_and(|active| root.contains(Some(&active)))
        });
        if !in_editor {
            return;
        }
        let button = self.editing.keep_mine_ref;
        // After the notice renders.
        if let Some(window) = web_sys::window() {
            let focus = wasm_bindgen::closure::Closure::once_into_js(move || {
                if let Some(button) = button.try_get_untracked().flatten() {
                    let _ = button.focus();
                }
            });
            let _ = window.request_animation_frame(focus.unchecked_ref());
        }
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
