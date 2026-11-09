//! Text that is a value's content wherever it is: the inside of a string
//! written over lines (a TOML `"""` string, a YAML quoted scalar, an HCL
//! heredoc, a YAML block scalar's lines) and an XML element's text. Its
//! whitespace is content, never formatting, so while the text does not
//! parse an edit there inside a lock is refused, whatever it types.

use std::ops::Range;

use super::super::super::highlight::ConstructKind;
use super::super::super::language::ConfigLanguage;
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
    let units = |byte: usize| text[..byte].encode_utf16().count();
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
    if behavior.closing_markup_len("</x>").is_some() {
        areas.extend(element_texts(text));
    }
    areas
}

/// The texts of XML elements that hold more than whitespace: from right
/// after a tag's `>` to right before the next `<`.
fn element_texts(text: &str) -> Vec<Range<usize>> {
    let mut areas = Vec::new();
    let mut units = 0;
    let mut in_tag = false;
    let mut start: Option<usize> = None;
    let mut blank = true;
    for character in text.chars() {
        match character {
            '<' => {
                if let Some(from) = start.take()
                    && !blank
                {
                    areas.push(from..units);
                }
                in_tag = true;
            }
            '>' if in_tag => {
                in_tag = false;
                start = Some(units + 1);
                blank = true;
            }
            _ if !in_tag && !character.is_whitespace() => blank = false,
            _ => {}
        }
        units += character.len_utf16();
    }
    areas
}

#[cfg(test)]
mod tests {
    use super::element_texts;

    #[test]
    fn element_texts_are_the_texts_holding_more_than_whitespace() {
        let text = "<a>\n  <b>x y</b>\n  <c> </c>\n<d>l1\nl2</d></a>";
        let found: Vec<&str> = element_texts(text)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(found, ["x y", "l1\nl2"]);
    }
}
