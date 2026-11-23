//! Go back / go forward (`Alt+←` / `Alt+→`, `goBack()` / `goForward()`):
//! every jump (see `jump`) first notes where the user was, the selection
//! and the scroll, so going back puts both back exactly. The places follow
//! the edits made since (see `trail`).
//!
//! The editors of a documents view share one history: a jump into another
//! document notes the place in the document it leaves (the view says so
//! with [`EditorState::note_leaving`] before it shows the other one), and
//! going back shows that document again.

mod trail;

use leptos::prelude::*;

use super::edit::Change;
use super::handle::EditorHandle;
use super::positioning::reveal_offset;
use super::state::{EditorState, directed_selection};
use crate::reactive::later::Alive;
pub(crate) use trail::Trail;
use trail::{Place, Spot};

/// One editor in a shared history, and the handle that shows it (a
/// documents view's, which selects its document first).
#[derive(Clone, Copy)]
struct Member {
    key: u64,
    state: EditorState,
    handle: Option<EditorHandle>,
}

/// A history of places, its own editor's or a documents view's editors'.
#[derive(Default)]
pub(crate) struct Navigation {
    trail: Trail,
    members: Vec<Member>,
    /// The place a jump into another editor leaves, noted by the view just
    /// before it shows that editor; the jump takes it in place of the
    /// shown editor's own place.
    leaving: Option<Spot>,
    last_key: u64,
}

/// An editor's link to its history.
#[derive(Clone, Copy)]
pub(super) struct BackForward {
    shared: StoredValue<Navigation>,
    key: u64,
}

impl BackForward {
    /// Joins `shared` (a documents view's), else a history of its own.
    pub(super) fn new(shared: Option<StoredValue<Navigation>>) -> Self {
        let shared = shared.unwrap_or_else(|| StoredValue::new(Navigation::default()));
        let key = shared
            .try_update_value(|navigation| {
                navigation.last_key += 1;
                navigation.last_key
            })
            .unwrap_or_default();
        Self { shared, key }
    }
}

/// Which way to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Back,
    Forward,
}

impl EditorState {
    /// Lists the editor in its history once it is made; `handle` shows it
    /// when going back to it from another editor.
    pub(super) fn join_history(self, handle: Option<EditorHandle>) {
        let link = self.back_forward();
        link.shared.update_value(|navigation| {
            navigation
                .members
                .retain(|member| member.state.is_alive() && member.key != link.key);
            navigation.members.push(Member {
                key: link.key,
                state: self,
                handle,
            });
        });
    }

    /// The place the user is at now; `None` before the text area mounts.
    fn spot_here(&self) -> Option<Spot> {
        let area = self.textarea()?;
        let (anchor, caret) = directed_selection(&area);
        Some(Spot {
            editor: self.back_forward().key,
            place: Place {
                anchor,
                caret,
                top: area.scroll_top(),
                left: area.scroll_left(),
            },
        })
    }

    /// Notes the place before a jump to bytes `start..end`: the place the
    /// view said a jump from another document leaves, else this editor's
    /// own (unless the jump goes where the selection already is).
    pub(super) fn note_jump(&self, start: super::text::TextSize, end: super::text::TextSize) {
        let link = self.back_forward();
        let leaving = link
            .shared
            .try_update_value(|navigation| navigation.leaving.take())
            .flatten();
        let from = leaving.or_else(|| {
            self.spot_here().filter(|here| {
                let (anchor, caret) = (here.place.anchor, here.place.caret);
                (anchor.min(caret), anchor.max(caret)) != (start, end)
            })
        });
        if let Some(from) = from {
            link.shared
                .update_value(|navigation| navigation.trail.leave(from));
        }
    }

    /// Notes this editor's place as the one the next jump leaves: a
    /// documents view is about to show another document's editor, which
    /// jumps.
    pub(crate) fn note_leaving(&self) {
        let here = self.spot_here();
        self.back_forward()
            .shared
            .update_value(|navigation| navigation.leaving = here);
    }

    /// Forgets a place noted with [`Self::note_leaving`] no jump took.
    pub(crate) fn forget_leaving(&self) {
        let _ = self
            .back_forward()
            .shared
            .try_update_value(Navigation::forget_leaving);
    }

    /// Moves the history's places in this editor through `change`; a
    /// change of language drops them.
    pub(super) fn follow_history(&self, change: &Change, switching: bool) {
        let link = self.back_forward();
        let _ = link.shared.try_update_value(|navigation| {
            if switching {
                navigation.trail.forget(link.key);
            } else {
                navigation.trail.follow(link.key, change);
            }
        });
    }

    /// Whether going `direction` would move.
    pub(crate) fn can_go(&self, direction: Direction) -> bool {
        let Some(here) = self.spot_here() else {
            return false;
        };
        let link = self.back_forward();
        link.shared
            .try_with_value(|navigation| {
                let usable = |spot: &Spot| navigation.member(spot.editor).is_some();
                match direction {
                    Direction::Back => navigation.trail.can_back(here, usable),
                    Direction::Forward => navigation.trail.can_forward(here, usable),
                }
            })
            .unwrap_or(false)
    }

    /// Goes back or forward: the place's editor shown, its selection and
    /// scroll as they were. Returns whether it moved.
    pub(crate) fn go(&self, direction: Direction) -> bool {
        let Some(here) = self.spot_here() else {
            return false;
        };
        let link = self.back_forward();
        let found = link
            .shared
            .try_update_value(|navigation| {
                let members = navigation.members.clone();
                let usable = |spot: &Spot| {
                    members
                        .iter()
                        .any(|member| member.key == spot.editor && member.state.is_alive())
                };
                let spot = match direction {
                    Direction::Back => navigation.trail.back(here, usable),
                    Direction::Forward => navigation.trail.forward(here, usable),
                }?;
                let member = navigation.member(spot.editor)?;
                Some((member, spot.place))
            })
            .flatten();
        let Some((member, place)) = found else {
            return false;
        };
        if member.key != link.key
            && let Some(handle) = member.handle
        {
            handle.show();
        }
        // Showing another document notes a leaving place: going back is no
        // jump, it takes none.
        self.forget_leaving();
        member.state.restore_place(place)
    }

    /// Puts `place` back: its selection, then its scroll exactly; only when
    /// edits since moved the caret out of that view does it scroll, as
    /// little as it takes. No flash: the view is as the user left it.
    fn restore_place(&self, place: Place) -> bool {
        let (start, end) = (place.anchor.min(place.caret), place.anchor.max(place.caret));
        let backward = place.caret < place.anchor;
        let Some(area) = self.focus_selection(start, end, backward) else {
            return false;
        };
        self.hide_shown_problem();
        area.set_scroll_top(place.top);
        area.set_scroll_left(place.left);
        self.doc()
            .text
            .with_untracked(|text| reveal_offset(&area, text, place.caret));
        true
    }
}

impl Navigation {
    /// Forgets the place a jump into another editor was to leave.
    pub(crate) fn forget_leaving(&mut self) {
        self.leaving = None;
    }

    /// The live editor listed as `key`.
    fn member(&self, key: u64) -> Option<Member> {
        self.members
            .iter()
            .find(|member| member.key == key && member.state.is_alive())
            .copied()
    }
}
