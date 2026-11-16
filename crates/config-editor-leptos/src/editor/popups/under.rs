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

/// How long the pointer rests under the popup before where it is decides,
/// in milliseconds: longer than the gap between two moves of a hand on
/// its way (8 to 16 ms, up to ~50 ms slowing down), short enough that a
/// click right after stopping on the popup reaches it.
pub(super) const SETTLE_MS: i32 = 120;

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

#[cfg(test)]
mod tests {
    use super::{Under, under};

    /// A pointer that came onto a card from beside it waits while it moves along the line it came in on; off that line
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
