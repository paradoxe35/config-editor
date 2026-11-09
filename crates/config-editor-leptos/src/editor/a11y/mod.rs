//! Screen-reader and keyboard access beyond the labelled parts:
//!
//! - `focus`: leaving the editor with the keyboard. Tab indents while
//!   editing, so `Ctrl+M` switches Tab to moving the focus (as in VS Code),
//!   and Tab or Shift+Tab right after an Escape that closed nothing leaves
//!   once.
//! - `caret_note`: what is at the caret (problems, a read-only reason, a
//!   secret's hint) in a hidden element the textarea's `aria-describedby`
//!   points at, updated a moment after the caret stops.
//! - `announce`: one polite live region for short messages (the Tab mode,
//!   "Error 2 of 5: …" on F8).
//! - `suggestions`: the suggestion list named on the textarea, with the
//!   highlighted suggestion as its active descendant.
//! - `texts`: how problems are worded for screen readers.

mod caret_note;
mod focus;
mod suggestions;
mod texts;

pub(super) use caret_note::{
    caret_position, caret_problem_docs, caret_problems_said, connect_caret_note, open_docs,
    open_docs_key, problems_at_caret, show_at_caret,
};
pub(super) use focus::{focus_keys, toggle_tab_focus};
pub(super) use suggestions::connect_suggestions;
pub(super) use texts::{hover_text, problem_announcement, problem_marker, severity_name};

use leptos::prelude::*;

/// The accessibility part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct A11yState {
    /// `Ctrl+M`: Tab and Shift+Tab move the focus instead of indenting.
    pub(super) tab_moves_focus: RwSignal<bool>,
    /// The last key was an Escape that had nothing to close: a Tab now
    /// leaves the editor once.
    pub(super) escaped: StoredValue<bool>,
    /// The live region's text.
    pub(super) announcement: RwSignal<String>,
    /// What is at the caret, for `aria-describedby`.
    pub(super) caret_note: RwSignal<String>,
    /// Bumped on each caret move, so only the last scheduled update lands.
    pub(super) note_token: StoredValue<u64>,
    /// Distinguishes this editor's elements from other editors'.
    pub(super) id: usize,
    /// Where the read-only editor's notice shows (see `note_read_only`).
    pub(super) read_only_notice: RwSignal<Option<usize>>,
    /// The timer that puts the notice away.
    pub(super) notice_timer: StoredValue<crate::reactive::later::Timeout>,
}

static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl A11yState {
    pub(super) fn new() -> Self {
        Self {
            tab_moves_focus: RwSignal::new(false),
            escaped: StoredValue::new(false),
            announcement: RwSignal::new(String::new()),
            caret_note: RwSignal::new(String::new()),
            note_token: StoredValue::new(0),
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            read_only_notice: RwSignal::new(None),
            notice_timer: StoredValue::new(Default::default()),
        }
    }

    /// The id of the element holding what is at the caret.
    pub(super) fn caret_note_id(&self) -> String {
        format!("config-editor-{}-caret-note", self.id)
    }

    /// Says `message` through the live region; the same message again is
    /// said again (a no-break space tells the two apart).
    pub(super) fn announce(&self, message: String) {
        let repeated = self
            .announcement
            .with_untracked(|shown| shown.trim_end_matches('\u{a0}') == message);
        let message = if repeated
            && !self
                .announcement
                .with_untracked(|shown| shown.ends_with('\u{a0}'))
        {
            format!("{message}\u{a0}")
        } else {
            message
        };
        self.announcement.set(message);
    }
}

#[cfg(test)]
mod tests;
