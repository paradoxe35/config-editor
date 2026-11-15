//! Classes of failure an open finding explains: the standing tests let
//! them through, naming the finding; its fix removes the entry (and the
//! test then holds the class out). Each class is a kind of failure at a
//! kind of step (`model::Act::kind`), in a view. All still hit on the
//! `host_locks` model (one record of the ranges as given per document).

use crate::model::Class;
use crate::run::View;

pub(crate) struct Known {
    pub(crate) views: &'static [View],
    pub(crate) class: Class,
    /// Whether it is the class at this kind of step.
    pub(crate) step: fn(&str) -> bool,
    /// The finding (qa38-findings.md), or why.
    pub(crate) finding: &'static str,
}

const BOTH: &[View] = &[View::Editor, View::Documents];

pub(crate) const KNOWN: &[Known] = &[
    // A reset (`replaceValue`, once or twice in a task) carries the
    // ranges by line: a lock sharing its line with text the reset changes
    // (the line above joined to it, say) lands on other text.
    Known {
        views: BOTH,
        class: Class::LockOff,
        step: |step| step.starts_with("Reset"),
        finding: "H38-1/H38-3 class: a lock carried into a reset's text lands off it",
    },
    // `update({ value })` without ranges after resets: the ranges carried
    // from the given ones land off the lock.
    Known {
        views: &[View::Editor],
        class: Class::LockOff,
        step: |step| step.starts_with("Own"),
        finding: "H38-2 class: a new value without ranges carries the lock off it",
    },
    // Ranges worked out in an older echo or `rangesFor` copy, moved to
    // the document by `remap`, land off the locked text when typing
    // beside it is in flight.
    Known {
        views: BOTH,
        class: Class::LockOff,
        step: |step| {
            step.starts_with("Deliver") || step.starts_with("RangesFor") || step == "drain"
        },
        finding: "FW38-3: ranges from a lagging copy land off the lock while echoes are in flight",
    },
    // `remap` grows a range over a whole changed stretch: an echo's ranges
    // with typing beside the lock, or a reset after typing, lock the lines
    // around it (typing there refused), and the same ranges given again
    // stay grown.
    Known {
        views: BOTH,
        class: Class::OverLockInFlight,
        step: |_| true,
        finding: "FW38-1 / H38-3: a moved range grows over the changed stretch",
    },
    Known {
        views: BOTH,
        class: Class::OverLockAtRest,
        step: |_| true,
        finding: "FW38-1 / H38-3: the grown lock stays once every text came back",
    },
    // NEW (FW38-2's class, without the binding): `replaceValue` with the
    // text the editor holds records nothing on its way; its listing,
    // coming after a newer text came back, is the host's own revert.
    Known {
        views: BOTH,
        class: Class::RevertedBySameListing,
        step: |_| true,
        finding: "NEW (FW38-2 class): a reset to the held text, listed late, reverts the typing",
    },
    // By the echo contract, not a finding: a host's own text equal to a
    // copy of the editor's still on its way is taken for its echo (seen
    // once in 20 000 runs of 60 steps: a reset, then a new version of the
    // file that removed the reset's line, as an older text was).
    Known {
        views: BOTH,
        class: Class::NotConverged,
        step: |_| true,
        finding: "by design: a host's own text equal to an echo on its way is that echo",
    },
];

/// The finding excusing `class` in `view` at `step`, if one does.
pub(crate) fn excused(view: View, class: Class, step: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| known.views.contains(&view) && known.class == class && (known.step)(step))
        .map(|known| known.finding)
}
