//! Aliases written out by a cut, back once the paste brings their anchor
//! back: a key holding an anchor (`port: &p 80`) cut in an Edit-as view
//! and saved writes each alias of it out (`port: 80`); pasted back above
//! them, the anchor is written again, and each value the session's first
//! text wrote as its alias that still reads as the anchor is its alias
//! again. So is an alias a save wrote out where the edit moved it with
//! its anchor: each value is found where the edit's links put it.

use super::aliases::shared_of;
use super::*;
use crate::editor::paths::{memo_key_span, memo_value_span, with_spans};
use crate::editor::text::{text_size};
use std::collections::HashMap;

/// `saved` with each value `origin` wrote as an alias (`*p`) of an anchor
/// `saved` writes before it, that `saved` writes out on its key's line
/// reading as the anchor, written as that alias again; `None` when there
/// is none, or when the text would not read the same. Each such value is
/// looked for where `placed` (the links: pointer in `saved`, pointer in
/// `origin`) puts it, and at its own pointer unless the links tell the
/// value there came from elsewhere.
pub(super) fn aliases_back(saved: &str, origin: &str, placed: &[(String, String)]) -> Option<String> {
    if !saved.contains('&') || !origin.contains('*') {
        return None;
    }
    let (_, was) = shared_of(origin);
    let (anchors, aliases) = shared_of(saved);
    let written: HashSet<&str> = aliases.iter().map(|alias| alias.pointer.as_str()).collect();
    let links = Links::of(placed);
    let candidates: Vec<(String, &String)> = was
        .iter()
        .filter(|alias| !alias.merge)
        .filter_map(|alias| Some((alias, anchors.get_key_value(&alias.name)?.0)))
        .flat_map(|(alias, name)| places(&alias.pointer, &links).into_iter().map(move |pointer| (pointer, name)))
        .filter(|(pointer, name)| !written.contains(pointer.as_str()) && anchors.get(*name) != Some(pointer))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let parsed = crate::editor::parse_cache::parsed(&YamlBehavior, saved);
    let value = parsed.as_ref().as_ref().ok()?;
    // Each place's spans, and its anchor's, read at once.
    let pointers: Vec<String> =
        candidates.iter().flat_map(|(pointer, name)| [pointer.clone(), anchors[*name].clone()]).collect();
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

/// Where `saved` holds the value `origin` holds at `pointer`: under each
/// item `placed` says came from it or from a map or list holding it, and
/// at its own pointer; each only where the links name no other source for
/// that place (the innermost item placed over it tells).
fn places(pointer: &str, links: &Links<'_>) -> Vec<String> {
    let source = |place: &str| {
        prefixes(place)
            .find_map(|(outer, rest)| Some(format!("{}{rest}", links.from.get(outer)?)))
            .unwrap_or_else(|| place.to_string())
    };
    let mut places: Vec<String> = prefixes(pointer)
        .flat_map(|(outer, rest)| {
            links.to.get(outer).into_iter().flatten().map(move |to| format!("{to}{rest}"))
        })
        .chain(std::iter::once(pointer.to_string()))
        .filter(|place| source(place) == pointer)
        .collect();
    places.sort();
    places.dedup();
    places
}

/// The links of a save's items, both ways: each item's source by its
/// pointer in the save, and the items placed from each source.
struct Links<'a> {
    from: HashMap<&'a str, &'a str>,
    to: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Links<'a> {
    fn of(placed: &'a [(String, String)]) -> Self {
        let mut links = Links { from: HashMap::new(), to: HashMap::new() };
        for (to, from) in placed {
            links.from.insert(to, from);
            links.to.entry(from.as_str()).or_default().push(to);
        }
        links
    }
}

/// `pointer` and each pointer holding it, innermost first, each with the
/// rest of `pointer` below it (`/a/b`: `/a/b` and ``, `/a` and `/b`, ``
/// and `/a/b`).
fn prefixes(pointer: &str) -> impl Iterator<Item = (&str, &str)> {
    let ends = pointer.rmatch_indices('/').map(|(at, _)| at);
    std::iter::once(pointer.len()).chain(ends).map(move |at| (&pointer[..at], &pointer[at..]))
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
        assert_eq!(aliases_back(&saved, &origin, &[]).as_deref(), Some(origin.as_str()));
        // Above its anchor, an alias would not read.
        assert_eq!(aliases_back("svc:\n  s: 80\nport: &p 80\n", "port: &p 80\nsvc:\n  s: *p\n", &[]), None);
        // A value that reads otherwise stays.
        let one_apart = aliases_back(&saved.replace("s1: 80", "s1: 81"), &origin, &[]);
        assert_eq!(one_apart.map(|text| text.matches("*p").count()), Some(2));
    }

    /// An alias goes back where the links put its value, not onto another
    /// value now at its old pointer: a map nested in a copy of its key
    /// keeps its alias of its own anchor, and the entries moved up beside
    /// it take theirs back.
    #[test]
    fn an_alias_goes_back_where_its_links_put_its_value() {
        let origin = "a:\n  k1:\n    base: &b1 5\n    copy: *b1\n  k3:\n    base: &b3 5\n    copy: *b3\n";
        let saved = "a:\n  k1:\n    k1:\n      base: &b1 5\n      copy: *b1\n    base: &b3 5\n    copy: 5\n";
        let placed = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs.iter().map(|(to, from)| (to.to_string(), from.to_string())).collect()
        };
        let links = placed(&[
            ("/a/k1/k1", "/a/k1"),
            ("/a/k1/base", "/a/k3/base"),
            ("/a/k1/copy", "/a/k3/copy"),
        ]);
        assert_eq!(
            aliases_back(saved, origin, &links).as_deref(),
            Some("a:\n  k1:\n    k1:\n      base: &b1 5\n      copy: *b1\n    base: &b3 5\n    copy: *b3\n")
        );
        // A map placed whole: its alias goes back under its new key.
        let moved = "a:\n  k1:\n    base: &b1 5\n    copy: *b1\n  k9:\n    base: &b3 5\n    copy: 5\n";
        assert_eq!(
            aliases_back(moved, origin, &placed(&[("/a/k9", "/a/k3")])).as_deref(),
            Some("a:\n  k1:\n    base: &b1 5\n    copy: *b1\n  k9:\n    base: &b3 5\n    copy: *b3\n")
        );
    }

    /// The places are read at once: the reads do not grow with the
    /// aliases brought back.
    #[test]
    fn the_reads_do_not_grow_with_the_aliases() {
        let reads = |count: usize| {
            let (origin, saved) = texts(count);
            [Work::DocumentRead, Work::PathLookup, Work::SpanLookup].map(|work| {
                counted(work, || assert_eq!(aliases_back(&saved, &origin, &[]).as_deref(), Some(origin.as_str()))).1
            })
        };
        assert_eq!(reads(20), reads(200));
    }
}
