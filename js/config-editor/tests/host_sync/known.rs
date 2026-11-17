//! Classes of failure an open finding or the design explains: the
//! standing tests let them through, naming the finding (or why it is by
//! design); a fix removes the entry (and the test then holds the class
//! out). Each class is a kind of failure at a kind of step
//! (`model::Act::kind`), in a view.

use crate::model::Class;
use crate::run::View;

pub(crate) struct Known {
    pub(crate) views: &'static [View],
    pub(crate) class: Class,
    /// Whether it is the class at this kind of step.
    pub(crate) step: fn(&str) -> bool,
    /// The finding, or why it is by design.
    pub(crate) finding: &'static str,
}

const BOTH: &[View] = &[View::Editor, View::Documents];

pub(crate) const KNOWN: &[Known] = &[
    // By the echo contract, not a finding: the text the host gave last,
    // given again, is no news (a framework's render passing an unchanged
    // prop). After a reset (`replaceValue`), a host whose own new version
    // of the file is that text again (it removed the reset's line) and that
    // drops the reset's listing leaves the editor on the reset's text and
    // itself on its own: seen about once in 20 000 runs of 60 steps. No
    // typing is lost. Only that case: the sims mark a host holding a text
    // of its own that is the one it gave last (`HostDoc::own`).
    Known {
        views: BOTH,
        class: Class::NotConvergedOnGivenAgain,
        step: |step| step == "drain",
        finding: "by design: the text a host gave last, given again, is no news",
    },
];

/// The finding excusing `class` in `view` at `step`, if one does.
pub(crate) fn excused(view: View, class: Class, step: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| known.views.contains(&view) && known.class == class && (known.step)(step))
        .map(|known| known.finding)
}
