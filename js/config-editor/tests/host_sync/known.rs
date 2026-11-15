//! Classes of failure an open finding explains: the standing tests let
//! them through, naming the finding; its fix removes the entry (and the
//! test then holds the class out). Each class is a kind of failure at a
//! kind of step (`model::Act::kind`), in a view.

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
