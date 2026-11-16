//! The popups over the text: the hover card (a key's docs), the problem
//! tip and the lock tip (what protected text is, or why an edit there was
//! refused). One owner, [`PopupState`]:
//!
//! - one popup is open at a time ([`Popup`]): the card or the problem tip;
//!   the problem tip F8 (or a tap) showed at the caret shows whenever none
//!   is (see `card_problems`). The lock tip is a note beside them, at its
//!   place in the text (a read-only key shows its docs and says why);
//! - the pointer's phase is one state machine (`machine`): closed →
//!   pending → open → leaving → closed, with hover intent and a safe
//!   polygon (`path`), as Floating UI's `useHover`/`safePolygon`;
//! - each timer belongs to the phase or popup that runs it: entering a new
//!   one cancels it.

mod card_problems;
mod caret_tip;
mod focus;
#[cfg(feature = "protect")]
mod lock;
mod machine;
mod open;
mod outside_press;
mod path;
mod pointer;
mod under;

use leptos::html::Div;
use leptos::prelude::*;

use super::components::HoverCard;
use super::state::EditorState;
use crate::reactive::later::{self, Timeout};

pub(super) use caret_tip::caret_tip_style;
pub(super) use focus::{card_keys, opens_docs_from, selects_all_in};
#[cfg(feature = "protect")]
pub(crate) use lock::LockTip;
pub(super) use outside_press::{close_popups_on_escape_over_them, close_popups_on_outside_press};
pub(super) use pointer::{back_on_text, enter_popup, list_wheel, popup_wheel};

use machine::{Machine, Opener};

/// The popup open.
#[derive(Clone)]
pub(crate) enum Popup {
    /// The hover card, listing the problems at its place.
    Card { card: Box<HoverCard>, by: Opener },
    /// The problem tip for the problems at one-based `place`, under the
    /// pointer or a focused underline.
    Tip { place: (usize, usize) },
}

/// The popups' part of the editor state.
#[derive(Clone, Copy)]
pub(crate) struct PopupState {
    /// The one popup open.
    open: RwSignal<Option<Popup>>,
    /// The pointer's phase and its timer.
    machine: RwSignal<Machine<HoverCard>>,
    /// The lock tip, and the timer a refusal's goes with.
    #[cfg(feature = "protect")]
    lock: RwSignal<Option<LockTip>>,
    #[cfg(feature = "protect")]
    lock_timer: StoredValue<Timeout>,
    /// The card's element while it shows: the keyboard reaches its links.
    pub(super) card_ref: NodeRef<Div>,
    /// The problem tip's element (a second `Ctrl+K Ctrl+I` focuses it).
    pub(super) tip_ref: NodeRef<Div>,
    /// The focus is in the popup shown: it is a dialog then, not a tooltip.
    pub(super) focused: RwSignal<bool>,
    /// The card's full height once measured (0 until then, unseen until
    /// then): it decides the card's side.
    pub(super) card_height: RwSignal<f64>,
    /// The problem tip takes the pointer: it has a docs link, or scrolls
    /// (said by the tip, which measures itself).
    pub(super) tip_reachable: RwSignal<bool>,
}

impl PopupState {
    pub(super) fn new() -> Self {
        Self {
            open: RwSignal::new(None),
            machine: RwSignal::new(Machine::default()),
            #[cfg(feature = "protect")]
            lock: RwSignal::new(None),
            #[cfg(feature = "protect")]
            lock_timer: StoredValue::new(Timeout::default()),
            card_ref: NodeRef::new(),
            tip_ref: NodeRef::new(),
            focused: RwSignal::new(false),
            card_height: RwSignal::new(0.0),
            tip_reachable: RwSignal::new(false),
        }
    }

    /// Whether the card or the problem tip is open (the caret's problem
    /// tip aside).
    pub(super) fn is_open(&self) -> bool {
        self.open.with_untracked(Option::is_some)
    }

    /// Whether anything Escape closes shows: the card, the tip, the lock
    /// tip (the caret's problem tip aside).
    pub(super) fn any_shown(&self) -> bool {
        #[cfg(feature = "protect")]
        if self.lock.with_untracked(Option::is_some) {
            return true;
        }
        self.is_open()
    }

    /// Opens `popup` in place of the one open. Without one in its place,
    /// the pointer has no popup to reach: its phase closes (a card waiting
    /// out its pause still waits).
    fn replace(&self, popup: Option<Popup>) {
        let closes = popup.is_none();
        let Some(old) = self.open.try_update(|open| std::mem::replace(open, popup)) else {
            return;
        };
        match old {
            Some(_) if closes => {
                self.machine.update(|machine| {
                    machine.closed();
                    // Closed runs no timer.
                    if let Some((before, _)) = machine.restart() {
                        before.cancel();
                    }
                });
            }
            _ => {}
        }
    }

    /// The card, while one is open (tracked).
    pub(super) fn card(&self) -> Option<HoverCard> {
        self.open.with(|open| match open {
            Some(Popup::Card { card, .. }) => Some(HoverCard::clone(card)),
            _ => None,
        })
    }

    /// Reads the card open, if any.
    pub(super) fn with_card<R>(&self, read: impl FnOnce(Option<&HoverCard>) -> R) -> R {
        self.open.with_untracked(|open| match open {
            Some(Popup::Card { card, .. }) => read(Some(card)),
            _ => read(None),
        })
    }

    /// Whether the card open was shown at the caret.
    pub(super) fn card_at_caret(&self) -> bool {
        self.open.with_untracked(|open| {
            matches!(
                open,
                Some(Popup::Card {
                    by: Opener::Caret,
                    ..
                })
            )
        })
    }

    /// Where the problem tip the pointer (or a focused underline) opened
    /// points, one-based.
    pub(super) fn hovered_problem(&self) -> Option<(usize, usize)> {
        self.open.with_untracked(|open| match open {
            Some(Popup::Tip { place }) => Some(*place),
            _ => None,
        })
    }

    /// Whether the popup shown lets the pointer through (tracked).
    pub(super) fn passing(&self) -> bool {
        self.machine.with(Machine::passing)
    }
}

impl EditorState {
    /// Runs `change` on the pointer's phase, then starts the timer of a
    /// phase it entered (the one before's cancelled).
    fn step<R>(&self, change: impl FnOnce(&mut Machine<HoverCard>) -> R) -> Option<R> {
        let machine = self.popups().machine;
        let (result, restart) = machine.try_update(|machine| {
            let result = change(machine);
            (result, machine.restart())
        })?;
        if let Some((before, delay)) = restart {
            before.cancel();
            if let Some(delay) = delay {
                let timer = later::after(*self, delay, EditorState::popup_timer_fired);
                // A timer that ran at once (no clock natively) left no handle.
                if timer != Timeout::default() {
                    machine.update_untracked(|machine| machine.set_timer(timer));
                }
            }
        }
        Some(result)
    }

    /// The pointer's phase, read.
    fn with_machine<R>(&self, read: impl FnOnce(&Machine<HoverCard>) -> R) -> R {
        self.popups().machine.with_untracked(read)
    }
}
