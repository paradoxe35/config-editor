//! A pointer that came onto a popup it can enter (the hover card, the
//! problem tip with a link) across its side, from the lines it covers
//! beside it (often after a detour: into the indentation or the gutter,
//! past the popup's far side), or another way than from what the popup is
//! for. Such a pointer may be heading for the popup or for a line the
//! popup covers, and only where it goes on to tells which. Meanwhile the popup stays (it showed
//! when the pointer got there, as VS Code's hiding delay keeps it) and
//! lets the pointer through: what it crosses waits, and a click places the
//! caret in the text under it. Then:
//!
//! - off the line it came in along, over the popup, it heads for the
//!   popup: the popup takes it;
//! - stopped on that line, on text there that shows something of its own
//!   (its docs, a problem), it went to that line: that shows, the popup
//!   goes;
//! - stopped anywhere else on the popup, it went to the popup: the popup
//!   takes it.

use leptos::prelude::*;

use super::popup_pointer::{lets_through, take_pointer_again};
use super::state::EditorState;
use crate::reactive::later;

/// How long the pointer rests under the popup before where it is decides,
/// in milliseconds: longer than the gap between two moves of a hand on
/// its way (8 to 16 ms, up to ~50 ms slowing down), short enough that a
/// click right after stopping on the popup reaches it.
const SETTLE_MS: i32 = 120;

/// What a pointer under a popup that lets it through does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Under {
    /// On its way along the line it came in on: wait for it to stop.
    Wait,
    /// Heading for the popup, or stopped on it: the popup takes it.
    Take,
    /// Stopped on the text of the line it came in along, which shows
    /// something of its own: that shows.
    GiveWay,
}

/// What the pointer under a popup does, on zero-based `line`, having come
/// in along `entered` (none: it just came), `stopped` there or still
/// moving, over text that `shows_own` docs or problems.
pub(super) fn under(entered: Option<usize>, line: usize, stopped: bool, shows_own: bool) -> Under {
    if entered.is_some_and(|entered| entered != line) {
        return Under::Take;
    }
    match (stopped, shows_own) {
        (false, _) => Under::Wait,
        (true, true) => Under::GiveWay,
        (true, false) => Under::Take,
    }
}

impl EditorState {
    /// The pointer moved over cell `cell` of zero-based `line`, under
    /// `popup`, which lets it through: the popup stays, and the text there
    /// waits (true) unless the pointer heads for the popup, which takes it.
    pub(super) fn under_popup(
        &self,
        popup: &web_sys::HtmlElement,
        line: usize,
        cell: usize,
    ) -> bool {
        let hover = self.hover();
        let entered = hover.under_line.get_value();
        if under(entered, line, false, false) == Under::Take {
            self.take_popup(popup);
            return true;
        }
        if entered.is_none() {
            hover.under_line.set_value(Some(line));
        }
        self.hold_popup();
        hover.under_token.update_value(|token| *token += 1);
        let token = hover.under_token.get_value();
        later::after(*self, SETTLE_MS, move |state| {
            if state.hover().under_token.get_value() == token {
                state.settle_under(line, cell);
            }
        });
        true
    }

    /// The pointer stopped under the popup, over cell `cell` of zero-based
    /// `line` (the line it came in along).
    fn settle_under(&self, line: usize, cell: usize) {
        let Some(popup) = self.reachable_popup_element() else {
            return;
        };
        if !lets_through(&popup) {
            return;
        }
        let entered = self.hover().under_line.get_value();
        match under(entered, line, true, self.shows_own_popup(line, cell)) {
            Under::Wait => {}
            Under::Take => self.take_popup(&popup),
            Under::GiveWay => {
                self.leave_under();
                self.hide_hover();
                self.hide_problem_tip();
                self.point_again();
            }
        }
    }

    /// The popup takes the pointer under it, as if it had come from what
    /// the popup is for: it enters the popup on its next move there (the
    /// page sends no events to a pointer at rest), and leaving straight
    /// from here is leaving the text.
    fn take_popup(&self, popup: &web_sys::HtmlElement) {
        self.leave_under();
        take_pointer_again(popup);
        let hover = self.hover();
        hover.on_way.set_value(true);
        hover.way_start.set_value(hover.pointer.get_value());
        self.hold_popup();
    }

    /// The pointer is not under a popup that lets it through (any more).
    pub(super) fn leave_under(&self) {
        let hover = self.hover();
        hover.under_line.set_value(None);
        hover.under_token.update_value(|token| *token += 1);
    }

    /// Whether cell `cell` of zero-based `line` shows something of its own
    /// to the pointer: docs other than the card's, problems other than the
    /// tip's.
    fn shows_own_popup(&self, line: usize, cell: usize) -> bool {
        if self.on_popup_anchor(line, cell) {
            return false;
        }
        let docs = self.config().settings.hover_docs.get_untracked()
            && self.docs_under_pointer(line, cell).is_some();
        docs || self.problem_place_at(line, cell).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{Under, under};

    /// QA 29 (M29-U1, L29-U2): a pointer that came onto a card from beside
    /// it waits while it moves along the line it came in on; off that line
    /// the card takes it; where it stops on that line decides: on text
    /// with docs or problems of its own, that text's; elsewhere, the
    /// card's.
    #[test]
    fn where_the_pointer_stops_under_a_popup_decides() {
        assert_eq!(under(None, 4, false, true), Under::Wait);
        assert_eq!(under(Some(4), 4, false, true), Under::Wait);
        assert_eq!(under(Some(4), 4, false, false), Under::Wait);
        assert_eq!(under(Some(4), 3, false, false), Under::Take);
        assert_eq!(under(Some(4), 5, false, true), Under::Take);
        assert_eq!(under(Some(4), 3, true, true), Under::Take);
        assert_eq!(under(Some(4), 4, true, true), Under::GiveWay);
        assert_eq!(under(None, 4, true, true), Under::GiveWay);
        assert_eq!(under(Some(4), 4, true, false), Under::Take);
    }
}
