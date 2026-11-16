//! The cases known to grow faster than the document, each naming why: an
//! open finding (its fix removes the entry and its ignored test in
//! `mod.rs`), or a real reason a kind of work grows so.

use super::Case;
use super::edits::EditKind;
use super::shapes::Shape;
use crate::editor::work_count::Work;

/// Cases an open finding makes grow: left out of the standing tests for
/// the kinds of work it names, and checked whole by its ignored test.
pub(super) struct Known {
    /// The finding (its ignored test's name says it too).
    pub(super) finding: &'static str,
    /// The kinds of work it makes grow (the total grows with them).
    pub(super) work: &'static [Work],
    pub(super) applies: fn(&Case) -> bool,
}

pub(super) const KNOWN: &[Known] = &[];

/// Cases a kind of work grows faster in by design, each with its reason
/// (`finding` names it): left out of the standing tests for the kinds of
/// work named, for good.
pub(super) const ALLOWED: &[Known] = &[
    // A Replace all over a view's items: the pairing that tells which
    // changed list items moved (`item_sources::pairing::by_values`) scores
    // every pair of items left unpaired while there are at most
    // `PAIRS_COMPARED` (250 000) such pairs; past that, each new item is
    // scored only against the unused old items nearest its place, as many
    // as that budget allows. So the work is quadratic up to the budget and
    // linear past it; following the items through the edit is linear
    // (`large_edit_work`).
    Known {
        finding: "list items paired up to a fixed budget",
        work: &[Work::ItemComparison],
        applies: |case| view(case) && kinds(case, &[EditKind::ReplaceAll]),
    },
    // A value nested in a recursive `anyOf`/`oneOf`: each value is
    // explained at most 256 times (`diagnostics::explanation`), so below
    // that bound the work follows the option tree and past it grows
    // linearly with the depth; `schema_error_cost` checks it at depths
    // 8, 12 and 16 (QA 37, SC37-2), too slow for this matrix's 25 setups.
    Known {
        finding: "options explained up to a fixed bound",
        work: &[Work::ErrorVisit, Work::OptionTried, Work::BranchWalk],
        applies: |case| kinds(case, &[EditKind::Validate]) && shapes(case, &[Shape::Deep]),
    },
    // A host's own text with a field's key written many times: the locks
    // read the path at the start of every line (`protect::scan::written`)
    // in a text of up to `MAX_LINES` (500) lines, and of every line naming
    // a protected key in a larger one, with the text read once for all of
    // them. The work named here still grows faster than the text in some
    // languages: in HCL each of those reads is a scan of the text from the
    // top at that line (`ScannedBytes`).
    Known {
        finding: "every line's path read below 500 lines",
        work: &[Work::OffsetSeek, Work::CharacterStep, Work::ScannedBytes],
        applies: |case| {
            !view(case) && shapes(case, &[Shape::Secrets]) && kinds(case, &[EditKind::Open])
        },
    },
];

/// The finding (or reason) excusing `work` in `case`, if one does.
pub(super) fn excused(case: &Case, work: Option<Work>) -> Option<&'static str> {
    KNOWN
        .iter()
        .chain(ALLOWED)
        .find(|known| (known.applies)(case) && work.is_none_or(|work| known.work.contains(&work)))
        .map(|known| known.finding)
}

/// Whether `case` is one of `finding`'s.
#[allow(
    dead_code,
    reason = "used by an open finding's ignored test (none open now)"
)]
pub(super) fn of(case: &Case, finding: &str) -> bool {
    KNOWN
        .iter()
        .any(|known| known.finding == finding && (known.applies)(case))
}

fn view(case: &Case) -> bool {
    case.view.is_some()
}

fn kinds(case: &Case, kinds: &[EditKind]) -> bool {
    kinds.contains(&case.kind)
}

fn shapes(case: &Case, shapes: &[Shape]) -> bool {
    shapes.contains(&case.shape)
}
