//! Classes of failure an open finding explains, each naming it (NEW: found
//! by this test, not yet in the findings list): the standing test lets
//! them through; the finding's fix removes the entry.

use super::Class;

pub(super) struct Known {
    pub(super) finding: &'static str,
    pub(super) applies: fn(&Class) -> bool,
}

pub(super) const KNOWN: &[Known] = &[];

/// The finding excusing `class`, if one does.
pub(super) fn excused(class: &Class) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| (known.applies)(class))
        .map(|known| known.finding)
}
