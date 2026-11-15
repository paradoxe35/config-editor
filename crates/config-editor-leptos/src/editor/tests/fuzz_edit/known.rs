//! Failure classes the random tests know of, each naming its finding:
//! reported (with its smallest case), not failed. A fix removes its entry
//! (and the run shows the class gone); an entry matching nothing costs
//! nothing, but goes when its finding is fixed.

/// A known class: the properties it shows in and a part of its class
/// name. Each has its smallest case in `repros` (ignored while open).
struct Known {
    properties: &'static [&'static str],
    class: &'static str,
    finding: &'static str,
}

const KNOWN: &[Known] = &[];

/// The finding excusing `class` in `property`, if one does.
pub(super) fn excused(property: &str, class: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| known.properties.contains(&property) && class.contains(known.class))
        .map(|known| known.finding)
}
