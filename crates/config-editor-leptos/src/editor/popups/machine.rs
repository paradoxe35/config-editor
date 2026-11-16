//! The pointer's phase for the popup it can reach (the hover card, the
//! problem tip): one state machine, as Floating UI's `useHover` with
//! `safePolygon`.
//!
//! closed → pending (the hover pause, the open timer) → open (which popup,
//! how the pointer stands to it) → leaving (the safe polygon: the way to the
//! popup, and the close timer) → closed. Each phase names the one timer it
//! runs ([`Phase::timer`]); entering a phase cancels the timer of the phase
//! before it, so a timer from an earlier phase never lands. Pure: the editor
//! tells it what the pointer did and where the popup is (see `pointer`), and
//! carries out what it answers.

use super::path::{
    Point, Rect, Spot, Way, beside, gets_closer, gets_farther, on_covered_lines, spot, way_after,
};
use super::under::{SETTLE_MS, Under, under};
use crate::reactive::later::Timeout;

/// The pause before the pointer resting on a key shows its card, in
/// milliseconds, so moving across keys does not flash cards; the card
/// stays as long after the pointer leaves its key.
pub(crate) const HOVER_DELAY_MS: i32 = 350;

/// How long the problem tip stays after the pointer leaves its underline,
/// in milliseconds: time to reach it.
const TIP_GRACE_MS: i32 = 200;

/// A popup the pointer can reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Card,
    Tip,
}

impl Kind {
    /// How long the popup stays after the pointer leaves what it is for.
    fn grace_ms(self) -> i32 {
        match self {
            Kind::Card => HOVER_DELAY_MS,
            Kind::Tip => TIP_GRACE_MS,
        }
    }
}

/// What opened a popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opener {
    /// The pointer on what it is for: it goes when the pointer leaves.
    Pointer,
    /// The caret (`Ctrl+K Ctrl+I`, F8, a tap): the pointer leaving keeps
    /// it, and it takes the pointer from anywhere.
    Caret,
}

/// What a leaving popup does when its moment is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Then {
    /// It closes.
    Close,
    /// It closes, and what the pointer stopped on shows (the pointer
    /// stopped on the way to it).
    CloseAndPointAgain,
}

/// How the pointer stands to the popup: the safe polygon's state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Reach {
    /// The pointer's last move was on the way to the popup: on what it is
    /// for, in its gap, or heading for it from there (see `path`).
    on_way: bool,
    /// Where the way started: the pointer's last place on what the popup
    /// is for, or in its gap.
    start: Option<Point>,
    /// Where (the window's x) the pointer was last on what the popup is
    /// for: the gap reaches over to there.
    anchor_x: Option<f64>,
    /// The popup lets the pointer through to the text under it (it came
    /// onto it another way; see `under`).
    passing: bool,
}

impl Reach {
    /// The reach of a popup just opened, the pointer at `pointer`: one the
    /// pointer opened is under it, on its way there.
    fn opened(by: Opener, pointer: Option<Point>) -> Self {
        match by {
            Opener::Pointer => Self {
                on_way: true,
                start: pointer,
                anchor_x: pointer.map(|(x, _)| x),
                passing: false,
            },
            Opener::Caret => Self::default(),
        }
    }
}

/// Where the pointer is while a popup is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum At {
    /// Over the text (or off it).
    Text,
    /// Under the popup, which lets it through, having come in along
    /// zero-based line `entered`; last over cell `cell` of `line`. Where
    /// it stops decides (see `under`).
    Under {
        entered: usize,
        line: usize,
        cell: usize,
    },
    /// In the popup.
    Inside,
}

/// The pointer's phase. `K` is what a pending card will show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Phase<K> {
    Closed,
    /// The pointer rests on a key: its card shows when the pause is over.
    /// A popup shown meanwhile (the problem tip under the pointer) keeps
    /// its own phase `behind`, without its timer: the card replaces it.
    Pending {
        key: K,
        behind: Option<Box<Phase<K>>>,
    },
    Open {
        kind: Kind,
        by: Opener,
        reach: Reach,
        at: At,
    },
    /// The pointer left what the popup is for: it closes when the moment
    /// is over, unless the pointer reaches it. Only a popup the pointer
    /// opened leaves.
    Leaving {
        kind: Kind,
        reach: Reach,
        then: Then,
    },
}

impl<K> Phase<K> {
    /// The phase's timer, in milliseconds.
    pub(crate) fn timer(&self) -> Option<i32> {
        match self {
            Phase::Pending { .. } => Some(HOVER_DELAY_MS),
            Phase::Leaving { kind, .. } => Some(kind.grace_ms()),
            Phase::Open {
                at: At::Under { .. },
                ..
            } => Some(SETTLE_MS),
            _ => None,
        }
    }

    fn reach_mut(&mut self) -> Option<&mut Reach> {
        match self {
            Phase::Open { reach, .. } | Phase::Leaving { reach, .. } => Some(reach),
            _ => None,
        }
    }
}

/// What a phase's timer does when it fires.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Fire<K> {
    /// The pause is over: the card for `K` opens.
    Open(K),
    /// The popup's moment is over.
    Close { kind: Kind, then: Then },
    /// The pointer stopped under the popup, over `cell` of `line`.
    Settle { line: usize, cell: usize },
}

/// What the text under a move does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Over {
    /// It waits: the pointer heads for the popup, is in its gap, or under
    /// it.
    Waits,
    /// It answers (shows its own docs or problems).
    Answers,
}

/// Where the pointer stopped under a popup took it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    /// To the popup, which takes it (or nothing to decide).
    Popup,
    /// To the text there: the popup goes, and that text shows.
    Text,
}

/// What the editor sees of the popup for a move over the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sight {
    /// The popup's box, in the window (none while not laid out).
    pub(crate) popup: Option<Rect>,
    /// The pointer is on what the popup is for: the card's key, the
    /// problems the tip shows.
    pub(crate) on_anchor: bool,
}

/// The pointer, for the popups.
#[derive(Clone, Debug)]
pub(crate) struct Machine<K> {
    phase: Phase<K>,
    /// Where the pointer last was, in the window's pixels.
    pointer: Option<Point>,
    /// The pointer is over the text, since its last move there until it
    /// left it.
    on_text: bool,
    /// The timer `phase` runs (the editor sets it, see [`Self::restart`]).
    timer: Timeout,
    /// `phase` was entered since the editor last started its timer.
    restart: bool,
}

impl<K> Default for Machine<K> {
    fn default() -> Self {
        Self {
            phase: Phase::Closed,
            pointer: None,
            on_text: false,
            timer: Timeout::default(),
            restart: false,
        }
    }
}

impl<K: Clone> Machine<K> {
    #[cfg(test)]
    pub(crate) fn phase(&self) -> &Phase<K> {
        &self.phase
    }

    pub(crate) fn pointer(&self) -> Option<Point> {
        self.pointer
    }

    pub(crate) fn on_text(&self) -> bool {
        self.on_text
    }

    /// Whether the popup shown lets the pointer through.
    pub(crate) fn passing(&self) -> bool {
        self.shown()
            .and_then(Phase::reach)
            .is_some_and(|reach| reach.passing)
    }

    /// Whether the pointer is in the popup.
    pub(crate) fn inside(&self) -> bool {
        matches!(self.shown(), Some(Phase::Open { at: At::Inside, .. }))
    }

    /// The card waiting out the pause.
    pub(crate) fn pending(&self) -> Option<&K> {
        match &self.phase {
            Phase::Pending { key, .. } => Some(key),
            _ => None,
        }
    }

    /// The timer of the phase before, to cancel, and the delay of the
    /// phase entered since (none: it runs none), once per entry.
    pub(crate) fn restart(&mut self) -> Option<(Timeout, Option<i32>)> {
        if !std::mem::take(&mut self.restart) {
            return None;
        }
        Some((std::mem::take(&mut self.timer), self.phase.timer()))
    }

    /// The timer just started for the phase.
    pub(crate) fn set_timer(&mut self, timer: Timeout) {
        self.timer = timer;
    }

    fn enter(&mut self, phase: Phase<K>) {
        self.phase = phase;
        self.restart = true;
    }

    /// The phase of the popup shown: the machine's, or the one behind a
    /// pending card.
    fn shown(&self) -> Option<&Phase<K>> {
        match &self.phase {
            Phase::Closed => None,
            Phase::Pending { behind, .. } => behind.as_deref(),
            phase => Some(phase),
        }
    }

    /// The shown popup's phase becomes `phase`: behind a pending card it
    /// waits without its timer.
    fn replace_shown(&mut self, phase: Phase<K>) {
        match &mut self.phase {
            Phase::Pending { behind, .. } => *behind = Some(Box::new(phase)),
            _ => self.enter(phase),
        }
    }

    fn reach(&self) -> Option<Reach> {
        self.shown().and_then(Phase::reach).copied()
    }

    fn set_reach(&mut self, new: Reach) {
        let shown = match &mut self.phase {
            Phase::Pending { behind, .. } => behind.as_deref_mut(),
            phase => Some(phase),
        };
        if let Some(reach) = shown.and_then(Phase::reach_mut) {
            *reach = new;
        }
    }

    /// The shown popup's kind and opener (a leaving one was opened by the
    /// pointer).
    fn shown_kind(&self) -> Option<(Kind, Opener)> {
        match self.shown()? {
            Phase::Open { kind, by, .. } => Some((*kind, *by)),
            Phase::Leaving { kind, .. } => Some((*kind, Opener::Pointer)),
            _ => None,
        }
    }

    /// A popup opened (`by` the pointer or the caret), the pointer
    /// wherever it was.
    pub(crate) fn opened(&mut self, kind: Kind, by: Opener) {
        let reach = Reach::opened(by, self.pointer);
        self.replace_shown(Phase::Open {
            kind,
            by,
            reach,
            at: At::Text,
        });
    }

    /// The popup shown closed (a card waiting out its pause still waits).
    pub(crate) fn closed(&mut self) {
        match &mut self.phase {
            Phase::Pending { behind, .. } => *behind = None,
            Phase::Closed => {}
            _ => self.enter(Phase::Closed),
        }
    }

    /// The pointer rests on a key: its card `key` shows after the pause.
    pub(crate) fn pend(&mut self, key: K) {
        let behind = match std::mem::replace(&mut self.phase, Phase::Closed) {
            Phase::Closed => None,
            Phase::Pending { behind, .. } => behind,
            phase => Some(Box::new(phase)),
        };
        self.enter(Phase::Pending { key, behind });
    }

    /// The pause over `key` is over, but there was nothing to show: it
    /// stays pending without a timer, so the pointer moving on the same
    /// field waits for nothing more (the popup shown meanwhile goes on).
    pub(crate) fn pend_quietly(&mut self, key: K) {
        let behind = match std::mem::replace(&mut self.phase, Phase::Closed) {
            Phase::Closed => None,
            Phase::Pending { behind, .. } => behind,
            phase => Some(Box::new(phase)),
        };
        // Entered without a restart: no timer runs.
        self.phase = Phase::Pending { key, behind };
    }

    /// The card waiting out its pause does not show after all; the popup
    /// shown meanwhile goes on (its timer again).
    pub(crate) fn cancel_pending(&mut self) {
        if let Phase::Pending { behind, .. } = &mut self.phase {
            let behind = behind.take().map_or(Phase::Closed, |phase| *phase);
            self.enter(behind);
        }
    }

    /// The popup stays: a moment it was given to be reached is called off.
    pub(crate) fn hold(&mut self) {
        if let Some(Phase::Leaving { kind, reach, .. }) = self.shown() {
            let (kind, reach) = (*kind, *reach);
            self.replace_shown(Phase::Open {
                kind,
                by: Opener::Pointer,
                reach,
                at: At::Text,
            });
        }
    }

    /// The pointer left what the popup `kind` is for (or the popup): it
    /// goes after its moment, unless the caret opened it or the pointer is
    /// in it. A moment already running is not put off.
    pub(crate) fn leave(&mut self, kind: Kind) {
        if let Some(Phase::Open {
            kind: shown,
            by: Opener::Pointer,
            reach,
            at,
        }) = self.shown()
            && *shown == kind
            && *at != At::Inside
        {
            let reach = *reach;
            self.replace_shown(Phase::Leaving {
                kind,
                reach,
                then: Then::Close,
            });
        }
    }

    /// The popup the pointer opened goes after its moment from now (`then`),
    /// unless the pointer gets there.
    fn leave_from_here(&mut self, then: Then) {
        let Some((kind, Opener::Pointer)) = self.shown_kind() else {
            return;
        };
        if self.inside() {
            return;
        }
        let Some(reach) = self.reach() else {
            return;
        };
        self.replace_shown(Phase::Leaving { kind, reach, then });
    }

    /// On the way to the popup: it stays a moment more, then goes unless
    /// the pointer got there (the card then lets what the pointer stopped
    /// on show).
    fn wait_on_the_way(&mut self) {
        match self.shown_kind() {
            Some((Kind::Card, _)) => self.leave_from_here(Then::CloseAndPointAgain),
            Some((Kind::Tip, _)) => self.leave_from_here(Then::Close),
            None => {}
        }
    }

    /// The pointer moved along the popup (no farther from it): it stays
    /// its moment from here, while what the pointer is on answers.
    fn hold_a_moment(&mut self) {
        self.leave_from_here(Then::Close);
    }

    /// The pointer moved to `to` over cell `cell` of zero-based `line`, a
    /// popup it can reach shown as `sight` says: whether the text there
    /// waits (the pointer heads for the popup across it) or answers.
    pub(crate) fn over_text(&mut self, to: Point, line: usize, cell: usize, sight: Sight) -> Over {
        let from = self.pointer.replace(to);
        let Some(mut reach) = self.reach() else {
            return Over::Answers;
        };
        let at = sight.popup.map_or(Spot::Away, |popup| {
            spot(to, popup, reach.passing, reach.anchor_x)
        });
        // Over the popup, which let the pointer through (it came another
        // way): where it goes on to decides.
        if at == Spot::Under {
            reach.on_way = false;
            self.set_reach(reach);
            self.under_popup(line, cell);
            return Over::Waits;
        }
        self.leave_under();
        // In the gap between the line and the popup: as good as on it, so
        // resting there keeps it.
        if at == Spot::Gap && !sight.on_anchor {
            reach.on_way = true;
            reach.start = Some(to);
            reach.passing = false;
            self.set_reach(reach);
            self.hold();
            return Over::Waits;
        }
        if sight.on_anchor {
            reach.on_way = true;
            reach.anchor_x = Some(to.0);
            reach.start = Some(to);
            reach.passing = false;
            self.set_reach(reach);
            return Over::Answers;
        }
        let way = match sight.popup {
            // Not laid out yet: a pointer at rest stays on its way.
            None => {
                let resting = reach.on_way && from == Some(to);
                Way {
                    heading: resting,
                    on: resting,
                    start: reach.start,
                }
            }
            Some(popup) => way_after(reach.on_way, from, reach.start, to, popup),
        };
        reach.on_way = way.on;
        reach.start = way.start;
        if sight
            .popup
            .is_some_and(|popup| !on_covered_lines(to, popup))
        {
            reach.passing = false;
        }
        self.set_reach(reach);
        let closer = sight
            .popup
            .is_some_and(|popup| gets_closer(from, to, popup));
        if !way.heading && !closer {
            if sight
                .popup
                .is_some_and(|popup| !gets_farther(from, to, popup))
            {
                self.hold_a_moment();
            }
            return Over::Answers;
        }
        self.wait_on_the_way();
        Over::Waits
    }

    /// The pointer moved to `to` off the text (the gutter, the page), the
    /// popup it can reach at `popup` (none: none laid out): as over the
    /// text, a way to it starts there off the lines it covers, and is lost
    /// beside it on them.
    pub(crate) fn off_text(&mut self, to: Point, popup: Option<Rect>) {
        self.leave_under();
        let from = self.pointer.replace(to);
        let Some(mut reach) = self.reach() else {
            return;
        };
        reach.anchor_x = None;
        let Some(popup) = popup else {
            reach.on_way = false;
            self.set_reach(reach);
            return;
        };
        let way = way_after(reach.on_way, from, reach.start, to, popup);
        reach.on_way = way.on;
        reach.start = way.start;
        if !on_covered_lines(to, popup) {
            reach.passing = false;
        }
        self.set_reach(reach);
        if way.heading || gets_closer(from, to, popup) {
            self.wait_on_the_way();
        } else if !gets_farther(from, to, popup) {
            self.hold_a_moment();
        }
    }

    /// The pointer left the text, at `to` onto the popup (`onto_popup`) or
    /// elsewhere: the next move goes on from there.
    pub(crate) fn left_text(&mut self, to: Point, onto_popup: bool, popup: Option<Rect>) {
        self.on_text = false;
        if !onto_popup {
            self.off_text(to, popup);
        }
    }

    /// The pointer moved over the text.
    pub(crate) fn on_text_again(&mut self) {
        self.on_text = true;
    }

    /// The pointer is on the problems the tip it opened shows: on the way
    /// to the tip.
    pub(crate) fn on_problems(&mut self) {
        if let Some(Phase::Open {
            kind: Kind::Tip,
            by: Opener::Pointer,
            ..
        }) = self.shown()
            && let Some(mut reach) = self.reach()
        {
            reach.on_way = true;
            self.set_reach(reach);
        }
    }

    /// The pointer moved under the popup, which lets it through, over cell
    /// `cell` of zero-based `line`: the popup stays; it takes the pointer
    /// when it heads off the line it came in along, else where it stops
    /// decides.
    fn under_popup(&mut self, line: usize, cell: usize) {
        let entered = match self.shown() {
            Some(Phase::Open {
                at: At::Under { entered, .. },
                ..
            }) => Some(*entered),
            _ => None,
        };
        if under(entered, line, false, false) == Under::Take {
            self.take_popup();
            return;
        }
        self.hold();
        if let Some(Phase::Open {
            kind, by, reach, ..
        }) = self.shown()
        {
            let (kind, by, reach) = (*kind, *by, *reach);
            self.replace_shown(Phase::Open {
                kind,
                by,
                reach,
                at: At::Under {
                    entered: entered.unwrap_or(line),
                    line,
                    cell,
                },
            });
        }
    }

    /// The pointer is no longer under the popup.
    fn leave_under(&mut self) {
        if let Some(Phase::Open {
            kind,
            by,
            reach,
            at: At::Under { .. },
        }) = self.shown()
        {
            let (kind, by, reach) = (*kind, *by, *reach);
            self.replace_shown(Phase::Open {
                kind,
                by,
                reach,
                at: At::Text,
            });
        }
    }

    /// The popup takes the pointer under it, as if it had come from what
    /// the popup is for: it enters the popup on its next move there (the
    /// page sends no events to a pointer at rest).
    fn take_popup(&mut self) {
        self.leave_under();
        if let Some(mut reach) = self.reach() {
            reach.passing = false;
            reach.on_way = true;
            reach.start = self.pointer;
            self.set_reach(reach);
        }
        self.hold();
    }

    /// The pointer stopped under the popup on zero-based `line`, over text
    /// that `shows_own` docs or problems: whether it went to the popup or
    /// to that text.
    pub(crate) fn settle(&mut self, line: usize, shows_own: bool) -> Settled {
        let Some(Phase::Open {
            at: At::Under { entered, .. },
            reach,
            ..
        }) = self.shown()
        else {
            return Settled::Popup;
        };
        if !reach.passing {
            return Settled::Popup;
        }
        match under(Some(*entered), line, true, shows_own) {
            Under::Wait => Settled::Popup,
            Under::Take => {
                self.take_popup();
                Settled::Popup
            }
            Under::GiveWay => {
                self.leave_under();
                Settled::Text
            }
        }
    }

    /// Whether the pointer comes onto `popup` across its side, from the
    /// lines it covers beside it (its last place over the text or the
    /// gutter): heading for it or not, it may be going to one of those
    /// lines.
    fn comes_from_beside(&self, popup: Option<Rect>) -> bool {
        self.pointer
            .zip(popup)
            .is_some_and(|(point, popup)| beside(point, popup))
    }

    /// The pointer came onto the popup at `popup`: whether it enters it (on
    /// its way from what the popup is for, or to a popup the caret
    /// opened). Otherwise the popup lets it through, and where it goes on
    /// to decides (see `under`); the pointer is over the text then.
    pub(crate) fn enter_popup(&mut self, popup: Option<Rect>) -> bool {
        let Some((kind, by)) = self.shown_kind() else {
            return false;
        };
        let Some(mut reach) = self.reach() else {
            return false;
        };
        let on_way = self.pointer.is_some() && reach.on_way && !self.comes_from_beside(popup);
        if by == Opener::Caret || on_way {
            self.replace_shown(Phase::Open {
                kind,
                by,
                reach,
                at: At::Inside,
            });
            return true;
        }
        reach.passing = true;
        self.set_reach(reach);
        self.leave_under();
        self.on_text = true;
        false
    }

    /// The pointer left the popup: whether it had entered it.
    pub(crate) fn left_popup(&mut self) -> bool {
        let Some(Phase::Open {
            kind,
            by,
            reach,
            at: At::Inside,
        }) = self.shown()
        else {
            return false;
        };
        let (kind, by, reach) = (*kind, *by, *reach);
        self.replace_shown(Phase::Open {
            kind,
            by,
            reach,
            at: At::Text,
        });
        true
    }

    /// The popup takes the pointer again (it shows something else).
    pub(crate) fn take_pointer_again(&mut self) {
        if let Some(mut reach) = self.reach()
            && reach.passing
        {
            reach.passing = false;
            self.set_reach(reach);
        }
    }

    /// The pointer stopped on the way and the popup went: what it rests on
    /// shows, as if it had just moved there. Whether it is over the text
    /// to do so.
    pub(crate) fn point_again(&mut self) -> bool {
        if let Some(mut reach) = self.reach() {
            reach.on_way = false;
            self.set_reach(reach);
        }
        self.on_text && self.pointer.is_some()
    }

    /// A leaving popup the editor keeps after all (the focus is in it).
    pub(crate) fn kept(&mut self) {
        self.hold();
    }

    /// The phase's timer fired: what it does. A card's pause ends here
    /// (the popup shown meanwhile goes on until the card replaces it).
    pub(crate) fn fired(&mut self) -> Option<Fire<K>> {
        match &mut self.phase {
            Phase::Pending { key, behind } => {
                let key = key.clone();
                let behind = behind.take().map_or(Phase::Closed, |phase| *phase);
                self.enter(behind);
                Some(Fire::Open(key))
            }
            Phase::Leaving { kind, then, .. } => Some(Fire::Close {
                kind: *kind,
                then: *then,
            }),
            Phase::Open {
                at: At::Under { line, cell, .. },
                ..
            } => Some(Fire::Settle {
                line: *line,
                cell: *cell,
            }),
            _ => None,
        }
    }
}

impl<K> Phase<K> {
    fn reach(&self) -> Option<&Reach> {
        match self {
            Phase::Open { reach, .. } | Phase::Leaving { reach, .. } => Some(reach),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
