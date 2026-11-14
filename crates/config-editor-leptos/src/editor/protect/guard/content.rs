//! Text that is a value's content wherever it is: the inside of a string
//! written over lines (a TOML `"""` string, a YAML quoted scalar, an HCL
//! heredoc, a YAML block scalar's lines) and an XML element's text. Its
//! whitespace is content, never formatting, so while the text does not
//! parse an edit there inside a lock is refused, whatever it types.

use std::ops::Range;

use super::super::super::codec::scalar_range;
use super::super::super::highlight::ConstructKind;
use super::super::super::language::ConfigLanguage;
use super::super::super::paths::written_value_span;
use super::super::locate::LineIndex;
use super::super::locks::Lock;
use super::edit::Edited;

/// Whether `edit`, leaving a text that does not parse, types onto an end
/// of one of `lock`'s stretches that lies in content (the last line of a
/// YAML block scalar), where whitespace is not free as it is next to a
/// value written on one line.
pub(super) fn glued_to_content(edit: &Edited<'_>, lock: &Lock) -> bool {
    let region = &edit.region;
    let ends: Vec<usize> = lock
        .ranges
        .iter()
        .flat_map(|range| [range.start, range.end])
        .filter(|end| *end == region.start || *end == region.end)
        .collect();
    if ends.is_empty() || edit.parsed().is_some() {
        return false;
    }
    let areas = edit.content_areas();
    ends.iter().any(|at| {
        areas
            .iter()
            .any(|area| area.start <= *at && *at <= area.end && in_lock(area, lock))
    })
}

/// Whether `edit`, leaving a text that does not parse, changes the inside
/// of `lock`'s value where it is a string written over lines (a YAML block
/// scalar, an HCL heredoc, a TOML multi-line string, XML text over lines),
/// as the language finds it written: its line breaks, blank lines and
/// indentation are the string's own, so a blank line typed between a block
/// scalar's header and its text changes it, as does a blank line taken
/// from after a block keeping its final line breaks (`|+`), even where the
/// line diff keeps every line of the lock. Text typed right at its edges
/// is left to the other checks; an edit leaving its written text whole
/// (Replace All elsewhere) keeps it.
pub(super) fn written_string_changed(edit: &Edited<'_>, lock: &Lock) -> bool {
    let Some(pointer) = lock.pointer.as_deref() else {
        return false;
    };
    // Read only for a lock on the lines of the edit, or right above them
    // (the blank line ending a kept block is past its stretch).
    let (Some(low), Some(high)) = (
        lock.ranges.iter().map(|range| range.start).min(),
        lock.ranges.iter().map(|range| range.end).max(),
    ) else {
        return false;
    };
    let lines = &edit.lines;
    if lines.line_of(edit.region.start) > lines.line_of(high) + 2
        || lines.line_of(edit.region.end) < lines.line_of(low)
    {
        return false;
    }
    let string = edit
        .protected
        .pins
        .iter()
        .any(|(known, value)| known == pointer && value.is_string());
    if !string {
        return false;
    }
    let Some(range) = written_value_span(edit.behavior, edit.before, pointer)
        .and_then(|span| scalar_range(edit.behavior, edit.before, span))
    else {
        return false;
    };
    let written = &edit.before[range.clone()];
    if !written.contains('\n') {
        return false;
    }
    // A value ending in a line break owns the one after it too (the blank
    // line a kept block's text ends with).
    let end = range.end
        + usize::from(written.ends_with('\n') && edit.before[range.end..].starts_with('\n'));
    let from = edit.lines.byte_of(edit.region.start);
    let to = edit.lines.byte_of(edit.region.end);
    // Space typed where a value ending in a line break ends (before XML
    // text's end tag, on the line after a kept block) is its text too.
    let spaced = || {
        edit.new[edit.new_region.clone()]
            .first()
            .and_then(|unit| char::from_u32(u32::from(*unit)))
            .is_some_and(char::is_whitespace)
    };
    let inside = if from == to {
        range.start < from && (from < end || (from == end && written.ends_with('\n') && spaced()))
    } else {
        from < end && range.start < to
    };
    // Its lines whole, from the first one's start to the last one's end
    // (the header, the closing line): a line break added at an edge of the
    // text inside keeps the text itself.
    let first = edit.before[..range.start]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let last = edit.before[end..]
        .find('\n')
        .map_or(edit.before.len(), |at| end + at);
    inside && !edit.after.contains(&edit.before[first..last])
}

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
fn in_lock(area: &Range<usize>, lock: &Lock) -> bool {
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
