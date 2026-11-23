//! The back and forward stacks of places, kept apart from the browser so
//! their rules are tested natively: a jump pushes where it left, going
//! back or forward swaps the place with the one it leaves, and an edit
//! moves every place of its editor with the text.

use super::super::edit::{Assoc, Change};
use super::super::text::TextSize;

/// The most places each stack keeps; the oldest go first.
pub(super) const CAP: usize = 50;

/// Where the user was: the selection (anchor, caret) and the scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(super) anchor: TextSize,
    pub(super) caret: TextSize,
    pub(super) top: i32,
    pub(super) left: i32,
}

impl Place {
    /// The same selection (the scroll aside).
    fn same_selection(&self, other: &Place) -> bool {
        self.anchor == other.anchor && self.caret == other.caret
    }

    /// The place in the text after `change`; `None` once text around its
    /// caret or anchor is gone. Text typed at it stays after it.
    pub(super) fn mapped(self, change: &Change) -> Option<Place> {
        let anchor = change.map_pos(self.anchor, Assoc::Before);
        let caret = change.map_pos(self.caret, Assoc::Before);
        (!anchor.deleted && !caret.deleted).then_some(Place {
            anchor: anchor.pos,
            caret: caret.pos,
            ..self
        })
    }
}

/// A place in one editor: `editor` is its key in the shared history (the
/// editors of a documents view share one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Spot {
    pub(super) editor: u64,
    pub(super) place: Place,
}

impl Spot {
    fn same(&self, other: &Spot) -> bool {
        self.editor == other.editor && self.place.same_selection(&other.place)
    }
}

/// The places to go back and forward to, the latest last.
#[derive(Debug, Default)]
pub(crate) struct Trail {
    back: Vec<Spot>,
    forward: Vec<Spot>,
}

impl Trail {
    /// A jump leaves `from`: it goes onto the back stack (in place of its
    /// top when that is the same selection), and the forward stack is
    /// cleared, as a new step in a browser's history clears it.
    pub(super) fn leave(&mut self, from: Spot) {
        self.forward.clear();
        push(&mut self.back, from);
    }

    /// Going back from `here`: the latest place `usable` takes that is not
    /// `here`, with `here` pushed onto the forward stack. Places skipped on
    /// the way are dropped.
    pub(super) fn back(&mut self, here: Spot, usable: impl Fn(&Spot) -> bool) -> Option<Spot> {
        step(&mut self.back, &mut self.forward, here, usable)
    }

    /// Going forward from `here`, as [`Self::back`] goes back.
    pub(super) fn forward(&mut self, here: Spot, usable: impl Fn(&Spot) -> bool) -> Option<Spot> {
        step(&mut self.forward, &mut self.back, here, usable)
    }

    /// Whether going back from `here` would move.
    pub(super) fn can_back(&self, here: Spot, usable: impl Fn(&Spot) -> bool) -> bool {
        self.back
            .iter()
            .any(|spot| usable(spot) && !spot.same(&here))
    }

    /// Whether going forward from `here` would move.
    pub(super) fn can_forward(&self, here: Spot, usable: impl Fn(&Spot) -> bool) -> bool {
        self.forward
            .iter()
            .any(|spot| usable(spot) && !spot.same(&here))
    }

    /// Moves the places of `editor` through `change`, dropping those whose
    /// text is gone.
    pub(super) fn follow(&mut self, editor: u64, change: &Change) {
        for stack in [&mut self.back, &mut self.forward] {
            stack.retain_mut(|spot| {
                if spot.editor != editor {
                    return true;
                }
                match spot.place.mapped(change) {
                    Some(place) => {
                        spot.place = place;
                        true
                    }
                    None => false,
                }
            });
        }
    }

    /// Drops every place of `editor` (its text changed language, or it is
    /// gone).
    pub(super) fn forget(&mut self, editor: u64) {
        self.back.retain(|spot| spot.editor != editor);
        self.forward.retain(|spot| spot.editor != editor);
    }

    /// Keeps only the places of the editors `listed` takes.
    pub(super) fn keep(&mut self, listed: impl Fn(u64) -> bool) {
        self.back.retain(|spot| listed(spot.editor));
        self.forward.retain(|spot| listed(spot.editor));
    }
}

/// Pushes `spot` onto `stack`, replacing a top with the same selection, and
/// keeping at most [`CAP`] places.
///
/// The first push takes room for all [`CAP`] places at once: a stack that
/// grew by doubling as it filled would keep a larger buffer after every few
/// dozen jumps, which reads as memory kept by each jump until the cap.
fn push(stack: &mut Vec<Spot>, spot: Spot) {
    if stack.last().is_some_and(|top| top.same(&spot)) {
        stack.pop();
    }
    if stack.len() == CAP {
        stack.remove(0);
    }
    if stack.capacity() < CAP {
        stack.reserve_exact(CAP - stack.len());
    }
    stack.push(spot);
}

/// Takes the latest usable place of `from` that is not `here`, pushing
/// `here` onto `to`.
fn step(
    from: &mut Vec<Spot>,
    to: &mut Vec<Spot>,
    here: Spot,
    usable: impl Fn(&Spot) -> bool,
) -> Option<Spot> {
    while let Some(spot) = from.pop() {
        if usable(&spot) && !spot.same(&here) {
            push(to, here);
            return Some(spot);
        }
    }
    None
}

#[cfg(test)]
#[path = "trail_tests.rs"]
mod tests;
