//! Broken saves of open findings (or of classes this test found, not
//! fixed yet), allowed by name so the standing test guards every other
//! kind. The first entry that applies names a failure; the shrinker keeps
//! a failure under its entry. Each fix removes its entry; the test then
//! fails on what the fix left. An entry named `decision:` is no finding
//! but a behaviour chosen on purpose, said in its name.

use super::{Broke, Broken, Case};

/// An open finding's broken saves.
struct Known {
    /// The finding (or the class id this test gave it).
    finding: &'static str,
    applies: fn(&Case, &Broke) -> bool,
}

const KNOWN: &[Known] = &[Known {
    finding: "decision: a container copied with its lines into a map or list the host writes on one line takes that line's inline form (keeping its lines would rewrite the line it lands in); the values inside keep their forms",
    applies: |_, broke| {
        kinds(broke, &[Broken::Form])
            && broke.failure.moved
            && broke.failure.was.ends_with('…')
            && !broke.failure.now.contains('…')
            && broke.failure.saved.lines().any(|line| {
                let line = line.trim();
                line.contains(broke.failure.now.as_str())
                    && !line.starts_with(broke.failure.now.as_str())
            })
    },
}];

/// The finding allowing `broke` in `case`, if one does.
pub(super) fn allowed(case: &Case, broke: &Broke) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| (known.applies)(case, broke))
        .map(|known| known.finding)
}

fn kinds(broke: &Broke, kinds: &[Broken]) -> bool {
    kinds.contains(&broke.failure.broken)
}
