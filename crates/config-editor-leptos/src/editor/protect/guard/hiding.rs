//! A comment or a string an edit opens that runs over a locked field (an
//! XML `<!--` inside a locked block, a TOML `"""` or a YAML quote above a
//! locked line): while the text does not parse, the field's lines would be
//! text inside it, hidden from the locks.

use super::super::super::keyboard::utf16_to_byte_index;
use super::super::super::language::ConfigLanguage;
use super::super::locate::LineIndex;

/// For each UTF-16 offset of `text` in `offsets`, whether it lies inside a
/// comment or a string written over several lines.
pub(super) fn hidden(behavior: &dyn ConfigLanguage, text: &str, offsets: &[usize]) -> Vec<bool> {
    if offsets.is_empty() {
        return Vec::new();
    }
    let spans: Vec<std::ops::Range<usize>> = behavior
        .comments(text)
        .into_iter()
        .chain(behavior.multiline_strings(text))
        .collect();
    let bodies = crate::editor::parse_cache::string_bodies(behavior, text);
    let lines = LineIndex::new(text);
    offsets
        .iter()
        .map(|offset| {
            let byte = utf16_to_byte_index(text, *offset);
            let line = lines.line_of(*offset);
            spans
                .iter()
                .any(|span| span.start < byte && byte < span.end)
                || bodies.iter().any(|body| body.contains(&line))
        })
        .collect()
}
