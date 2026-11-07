//! The hover card: schema docs for the key under the pointer, shown after a
//! short pause (see `input::mouse_move`) and hidden as the pointer leaves.

use leptos::prelude::*;

use super::components::HoverCard;
use super::state::EditorState;

/// The hover part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct HoverState {
    pub(super) card: RwSignal<Option<HoverCard>>,
    /// The key waiting out the pause: (line, start column, key).
    pub(super) pending: StoredValue<Option<(usize, usize, String)>>,
    /// The pointer is over the hover card (one with a link it can reach).
    pub(super) card_entered: StoredValue<bool>,
    /// Bumped on each hide, so a card scheduled before it never shows.
    pub(super) token: StoredValue<u64>,
}

impl HoverState {
    pub(super) fn new() -> Self {
        Self {
            card: RwSignal::new(None),
            pending: StoredValue::new(None),
            card_entered: StoredValue::new(false),
            token: StoredValue::new(0),
        }
    }
}

impl EditorState {
    /// Hides a card the pointer may be on its way to (one with a link)
    /// after a moment, unless it gets there; hides any other card now.
    pub(super) fn leave_hover(&self) {
        let reachable = self.hover.card.with_untracked(|card| {
            card.as_ref()
                .is_some_and(|card| card.info.docs_url.is_some())
        });
        if !reachable {
            self.hide_hover();
            return;
        }
        let token = self.hover.token.get_value();
        super::positioning::schedule_hover(*self, move |state| {
            if state.hover.token.get_value() == token && !state.hover.card_entered.get_value() {
                state.hide_hover();
            }
        });
    }

    pub(super) fn hide_hover(&self) {
        self.hover.token.update_value(|token| *token += 1);
        self.hover.pending.set_value(None);
        self.hover.card_entered.set_value(false);
        if self.hover.card.with_untracked(Option::is_some) {
            self.hover.card.set(None);
        }
    }
}
