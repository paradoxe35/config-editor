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

pub(super) const KNOWN: &[Known] = &[
    // An edit in an Edit-as view removing a line break is traced through
    // the item trail: each item after it found among those before by a
    // walk over them, each cut item's lines looked for over the whole
    // pasted text and every place already taken (quadratic; cubic with
    // items alike); and each item left unmatched (all of them after a
    // Replace all of their keys) compared with every item cut.
    Known {
        finding: "SP37-1",
        work: &[Work::TrailStep, Work::ItemComparison],
        applies: |case| {
            view(case)
                && kinds(
                    case,
                    &[
                        EditKind::DeleteLine,
                        EditKind::CutPausePaste,
                        EditKind::CutPaste,
                        EditKind::MoveLine,
                        EditKind::ReplaceAll,
                        EditKind::PasteWhole,
                    ],
                )
        },
    },
    // The errors of a recursive `anyOf` weighed again at each level
    // (exponential in the depth), the schema's branches walked for each.
    Known {
        finding: "SC37-2",
        work: &[Work::ErrorVisit, Work::BranchWalk],
        applies: |case| kinds(case, &[EditKind::Validate]) && shapes(case, &[Shape::Deep]),
    },
];

/// Cases a kind of work grows faster in by design, each with its reason
/// (`finding` names it): left out of the standing tests for the kinds of
/// work named, for good.
pub(super) const ALLOWED: &[Known] = &[
    // A host's own text with a field's key written many times: the locks
    // read the path at the start of every line (`protect::scan::written`),
    // each from the top of the text, in a text of up to `MAX_LINES` (500)
    // lines; a larger one gets a fixed budget of such reads, so the work
    // stops growing faster than the text past that size. The matrix's
    // texts are well under it.
    Known {
        finding: "every line's path read below 500 lines",
        work: &[Work::OffsetSeek, Work::CharacterStep],
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
