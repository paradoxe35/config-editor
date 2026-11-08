//! The hover card: schema docs for the key under the pointer, shown after a
//! short pause (see `input::mouse_move`) and hidden as the pointer leaves.

use leptos::prelude::*;

use super::a11y::{caret_position, hover_text};
use super::completion::key_hover;
use super::components::HoverCard;
use super::positioning::hover_card_style;
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
    /// The card was shown at the caret (`Ctrl+K Ctrl+I`, a tap), not by
    /// the pointer: the pointer leaving does not close it.
    pub(super) at_caret: StoredValue<bool>,
}

impl HoverState {
    pub(super) fn new() -> Self {
        Self {
            card: RwSignal::new(None),
            pending: StoredValue::new(None),
            card_entered: StoredValue::new(false),
            token: StoredValue::new(0),
            at_caret: StoredValue::new(false),
        }
    }
}

impl EditorState {
    /// Hides a card the pointer may be on its way to (one with a link)
    /// after a moment, unless it gets there; hides any other card now.
    pub(super) fn leave_hover(&self) {
        if self.hover().at_caret.get_value() {
            return;
        }
        let reachable = self.hover().card.with_untracked(|card| {
            card.as_ref()
                .is_some_and(|card| card.info.docs_url.is_some())
        });
        if !reachable {
            self.hide_hover();
            return;
        }
        let token = self.hover().token.get_value();
        super::positioning::schedule_hover(*self, move |state| {
            if state.hover().token.get_value() == token && !state.hover().card_entered.get_value() {
                state.hide_hover();
            }
        });
    }

    /// Shows the hover card for the key (or documented value) at the caret,
    /// as the pointer would, and says its docs to screen readers: the
    /// keyboard's way (`Ctrl+K Ctrl+I`) and a tap's to the docs. Returns
    /// whether there were docs to show.
    pub(super) fn show_hover_at_caret(&self) -> bool {
        if !self.config().settings.hover_docs.get_untracked() {
            return false;
        }
        let Some(element) = self.textarea() else {
            return false;
        };
        let caret = self.doc().caret.get_untracked();
        let found = self.doc().text.with_untracked(|text| {
            let (line, column) = caret_position(text, caret);
            let (line, column) = (line - 1, column - 1);
            self.config().schema.with_value(|schema| {
                // On the key, or just past its end.
                [Some(column), column.checked_sub(1)]
                    .into_iter()
                    .flatten()
                    .find_map(|at| key_hover(text, line, at, schema.as_deref(), self.behavior()))
                    .map(|info| (line, info))
            })
        });
        let Some((line, info)) = found else {
            return false;
        };
        self.hide_hover();
        let said = self.strings_untracked(|strings| hover_text(strings, &info));
        let style = hover_card_style(&element, line, info.start_column);
        self.hover().card.set(Some(HoverCard { info, line, style }));
        self.hover().at_caret.set_value(true);
        self.a11y().announce(said);
        true
    }

    pub(super) fn hide_hover(&self) {
        self.hover().token.update_value(|token| *token += 1);
        self.hover().pending.set_value(None);
        self.hover().card_entered.set_value(false);
        self.hover().at_caret.set_value(false);
        if self.hover().card.with_untracked(Option::is_some) {
            self.hover().card.set(None);
        }
    }
}
