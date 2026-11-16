//! Text that is a value's content wherever it is: the inside of a string
//! written over lines (a TOML `"""` string, a YAML quoted scalar, an HCL
//! heredoc, a YAML block scalar's lines) and an XML element's text. Its
//! whitespace is content, never formatting: a span holding it keeps it,
//! and whitespace written at a span's end there is glued onto it.

use std::ops::Range;

use super::super::super::highlight::ConstructKind;
use super::super::super::language::ConfigLanguage;
use super::super::locate::LineIndex;
use super::super::locks::Lock;

/// Whether the whitespace between UTF-16 `start` and `end` of the text
/// before is in content that lies in a lock.
pub(super) fn in_locked_content(
    areas: &[Range<usize>],
    locks: &[Lock],
    start: usize,
    end: usize,
) -> bool {
    areas.iter().any(|area| {
        area.start <= end && start <= area.end && locks.iter().any(|lock| in_lock(area, lock))
    })
}

/// Whether `area` is `lock`'s own content: it starts in one of the
/// lock's stretches (a string left open above a lock, running over it,
/// is not the locked value's).
pub(super) fn in_lock(area: &Range<usize>, lock: &Lock) -> bool {
    lock.ranges
        .iter()
        .any(|range| range.start <= area.start && area.start < range.end)
}

/// UTF-16 positions of `text` where whatever is typed is content, as
/// inclusive ranges (`start..end` holds `end`).
pub(super) fn content_areas(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
) -> Vec<Range<usize>> {
    // From the lines (a count from the top for each area was a pass of
    // the text per XML element: an Enter in a long broken text with locks
    // took half a second).
    let units = |byte: usize| lines.units_of(byte);
    let mut areas: Vec<Range<usize>> = behavior
        .multiline_constructs(text)
        .into_iter()
        .filter(|construct| matches!(construct.kind, ConstructKind::Colored(_)))
        .filter(|construct| construct.range.end <= text.len())
        .map(|construct| {
            let (start, end) = (units(construct.range.start), units(construct.range.end));
            // Strictly inside: the space before an opening quote or after
            // a closing one is not the string's.
            start + 1..end.saturating_sub(1).max(start + 1)
        })
        .collect();
    for body in crate::editor::parse_cache::string_bodies(behavior, text)
        .iter()
        .cloned()
    {
        if body.start < body.end {
            areas.push(lines.line_bounds(body.start).start..lines.line_bounds(body.end - 1).end);
        }
    }
    areas.extend(
        behavior
            .content_texts(text)
            .into_iter()
            .map(|range| units(range.start)..units(range.end)),
    );
    areas
}
