//! Text that is a value's content wherever it is: the inside of a string
//! written over lines (a TOML `"""` string, a YAML quoted scalar, an HCL
//! heredoc, a YAML block scalar's lines) and an XML element's text. Its
//! whitespace is content, never formatting: a span holding it keeps it,
//! and whitespace written at a span's end there is glued onto it.

use super::super::super::highlight::ConstructKind;
use super::super::super::language::ConfigLanguage;
use super::super::super::text::{TextRange, TextSize, byte_range};
use super::super::locate::LineIndex;
use super::super::locks::Lock;

/// Whether the whitespace between bytes `start` and `end` of the text
/// before is in content that lies in a lock.
pub(super) fn in_locked_content(
    areas: &[TextRange],
    locks: &[Lock],
    start: TextSize,
    end: TextSize,
) -> bool {
    areas.iter().any(|area| {
        area.start() <= end && start <= area.end() && locks.iter().any(|lock| in_lock(area, lock))
    })
}

/// Whether `area` is `lock`'s own content: it starts in one of the
/// lock's stretches (a string left open above a lock, running over it,
/// is not the locked value's).
pub(super) fn in_lock(area: &TextRange, lock: &Lock) -> bool {
    lock.ranges.iter().any(|range| range.contains(area.start()))
}

/// The places of `text` where whatever is typed is content, as inclusive
/// ranges (an area holds its end).
pub(super) fn content_areas(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
) -> Vec<TextRange> {
    let mut areas: Vec<TextRange> = behavior
        .multiline_constructs(text)
        .into_iter()
        .filter(|construct| matches!(construct.kind, ConstructKind::Colored(_)))
        .filter(|construct| construct.range.end <= text.len())
        .map(|construct| {
            let range = byte_range(construct.range);
            // Strictly inside: the space before an opening quote or after
            // a closing one is not the string's (a quote is one byte).
            let one = TextSize::from(1);
            let start = range.start() + one;
            let end = range.end().checked_sub(one).unwrap_or_default();
            TextRange::new(start, end.max(start))
        })
        .collect();
    for body in crate::editor::parse_cache::string_bodies(behavior, text)
        .iter()
        .cloned()
    {
        if body.start < body.end {
            areas.push(TextRange::new(
                lines.line_bounds(body.start).start(),
                lines.line_bounds(body.end - 1).end(),
            ));
        }
    }
    areas.extend(behavior.content_texts(text).into_iter().map(byte_range));
    areas
}
