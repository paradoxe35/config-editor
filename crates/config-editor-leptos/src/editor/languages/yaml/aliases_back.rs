//! Aliases written out by a cut, back once the paste brings their anchor
//! back: a key holding an anchor (`port: &p 80`) cut in an Edit-as view
//! and saved writes each alias of it out (`port: 80`); pasted back above
//! them, the anchor is written again, and each value the session's first
//! text wrote as its alias that still reads as the anchor is its alias
//! again.

use super::aliases::shared_of;
use super::*;
use crate::editor::paths::{memo_key_span, memo_value_span, with_spans};
use crate::editor::text::{text_size};

/// `saved` with each value `origin` wrote as an alias (`*p`) of an anchor
/// `saved` writes before it, that `saved` writes out on its key's line
/// reading as the anchor, written as that alias again; `None` when there
/// is none, or when the text would not read the same.
pub(super) fn aliases_back(saved: &str, origin: &str) -> Option<String> {
    if !saved.contains('&') || !origin.contains('*') {
        return None;
    }
    let (_, was) = shared_of(origin);
    let (anchors, aliases) = shared_of(saved);
    let written: HashSet<&str> = aliases.iter().map(|alias| alias.pointer.as_str()).collect();
    let candidates: Vec<(&String, &String)> = was
        .iter()
        .filter(|alias| !alias.merge && !written.contains(alias.pointer.as_str()))
        .filter_map(|alias| Some((&alias.pointer, anchors.get_key_value(&alias.name)?.0)))
        .filter(|(pointer, name)| anchors.get(*name) != Some(*pointer))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let parsed = crate::editor::parse_cache::parsed(&YamlBehavior, saved);
    let value = parsed.as_ref().as_ref().ok()?;
    // Each place's spans, and its anchor's, read at once.
    let pointers: Vec<String> =
        candidates.iter().flat_map(|(pointer, name)| [(*pointer).clone(), anchors[*name].clone()]).collect();
    let mut back: Vec<(std::ops::Range<usize>, String)> = with_spans(&YamlBehavior, saved, &pointers, || {
        candidates
            .iter()
            .filter_map(|(pointer, name)| {
                let anchor = &anchors[*name];
                (value.pointer(pointer)? == value.pointer(anchor)?).then_some(())?;
                let range = written_out(saved, pointer)?;
                // An alias reads only an anchor written before it.
                let anchor_line = memo_key_span(&YamlBehavior, saved, anchor)?.line;
                let line = crate::editor::paths::line_column(saved, text_size(range.start)).0;
                (anchor_line < line).then(|| (range, format!("*{name}")))
            })
            .collect()
    });
    if back.is_empty() {
        return None;
    }
    back.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut text = saved.to_string();
    for (range, alias) in back {
        text.replace_range(range, &alias);
    }
    let reads = crate::editor::parse_cache::parsed(&YamlBehavior, &text);
    (reads.as_ref().as_ref().ok()? == value).then_some(text)
}

/// The byte range of the value at `pointer`, written on its key's line
/// (not a block scalar or a block below it).
fn written_out(source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
    let span = memo_value_span(&YamlBehavior, source, pointer)?;
    if memo_key_span(&YamlBehavior, source, pointer)?.line != span.line {
        return None;
    }
    let range: std::ops::Range<usize> =
        crate::editor::codec::span_text_range(source, span)?.into();
    let text = &source[range.clone()];
    (!text.trim().is_empty() && !text.starts_with(['|', '>', '*', '&', '!'])).then_some(range)
}

#[cfg(test)]
mod tests {
    use super::aliases_back;
    use crate::editor::work_count::{Work, counted};

    /// The host before the cut (`origin`) and as the paste saves it,
    /// with `count` aliases.
    fn texts(count: usize) -> (String, String) {
        let entries = |value: &str| (0..count).map(|index| format!("  s{index}: {value} # c{index}\n")).collect::<String>();
        let origin = format!("port: &p 80\nsvc:\n{}", entries("*p"));
        let saved = format!("port: &p 80\nsvc:\n{}", entries("80"));
        (origin, saved)
    }

    #[test]
    fn values_written_out_are_aliases_again_below_their_anchor() {
        let (origin, saved) = texts(3);
        assert_eq!(aliases_back(&saved, &origin).as_deref(), Some(origin.as_str()));
        // Above its anchor, an alias would not read.
        assert_eq!(aliases_back("svc:\n  s: 80\nport: &p 80\n", "port: &p 80\nsvc:\n  s: *p\n"), None);
        // A value that reads otherwise stays.
        let one_apart = aliases_back(&saved.replace("s1: 80", "s1: 81"), &origin);
        assert_eq!(one_apart.map(|text| text.matches("*p").count()), Some(2));
    }

    /// The places are read at once: the reads do not grow with the
    /// aliases brought back.
    #[test]
    fn the_reads_do_not_grow_with_the_aliases() {
        let reads = |count: usize| {
            let (origin, saved) = texts(count);
            [Work::DocumentRead, Work::PathLookup, Work::SpanLookup].map(|work| {
                counted(work, || assert_eq!(aliases_back(&saved, &origin).as_deref(), Some(origin.as_str()))).1
            })
        };
        assert_eq!(reads(20), reads(200));
    }
}
