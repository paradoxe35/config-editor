//! Copies: a map entry no line of the edits tells, written in lines
//! pasted (see `pasted`) and holding the data of an entry followed or cut
//! under the same key in another map, is a copy of that entry (or the
//! second paste of one cut), and takes its source: it is written in the
//! form the host writes that entry in (QA `E37-5`: a copied TOML
//! datetime was written as a string, an HCL object attribute as a block,
//! XML attributes as child elements).

use super::super::super::work_count::{Work, note};
use super::Source;
use super::by_data::value_hash;
use super::items::Item;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Range;

/// The pointer of a map or list holding `pointer`.
fn parent(pointer: &str) -> &str {
    pointer.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// Each of `unmatched` (in document order) that is a copy, with the
/// source it takes: of `known` (entries followed or cut: their keys and
/// sources), the first holding its data (`value_of`) in another map,
/// looked up by its key and a hash of its data (a large paste compares
/// each entry with those holding the same only). Entries inside a copy
/// are that copy's.
pub(super) fn of<'i, 'v>(
    unmatched: &[&'i Item],
    pasted: &[Range<usize>],
    known: &[(&str, &Source)],
    now: &Value,
    value_of: impl Fn(&Source) -> Option<&'v Value>,
) -> Vec<(&'i str, Source)> {
    // The pasted ranges, which never overlap, by where they start.
    let mut pasted = pasted.to_vec();
    pasted.sort_by_key(|range| range.start);
    let in_pasted = |item: &Item| {
        let after = pasted.partition_point(|range| range.start <= item.lines.start);
        after > 0 && item.lines.end <= pasted[after - 1].end
    };
    // The entries known by key and data's hash, and by where they were in
    // the last saved view, made once one is wanted.
    type Known<'k> = (
        HashMap<(&'k str, u64), Vec<&'k Source>>,
        HashMap<&'k str, &'k Source>,
    );
    let mut lookup: Option<Known<'_>> = None;
    let mut out: Vec<(&'i str, Source)> = Vec::new();
    for item in unmatched {
        let Some(key) = item.key.as_deref() else {
            continue;
        };
        let inside = out.last().is_some_and(|(copy, _)| {
            item.pointer
                .strip_prefix(copy)
                .is_some_and(|rest| rest.starts_with('/'))
        });
        if inside || !in_pasted(item) {
            continue;
        }
        let Some(value) = now.pointer(&item.pointer) else {
            continue;
        };
        let (by_data, by_place) = lookup.get_or_insert_with(|| {
            let (mut by_data, mut by_place) = (HashMap::new(), HashMap::new());
            for &(key, source) in known {
                if let Some(held) = value_of(source) {
                    let entries: &mut Vec<&Source> =
                        by_data.entry((key, value_hash(held))).or_default();
                    entries.push(source);
                }
                if let Source::Synced(at) = source {
                    by_place.insert(at.as_str(), source);
                }
            }
            (by_data, by_place)
        });
        // Where an entry holding the same was: that entry, rewritten in
        // place (a large paste over the text), no copy.
        if by_place.get(item.pointer.as_str()).is_some_and(|source| {
            note(Work::ItemComparison);
            value_of(source) == Some(value)
        }) {
            continue;
        }
        let candidates = by_data.get(&(key, value_hash(value)));
        let source = candidates.into_iter().flatten().find(|source| {
            let from = match source {
                Source::Synced(from) | Source::Origin(from) => from,
            };
            parent(from) != parent(&item.pointer) && {
                note(Work::ItemComparison);
                value_of(source) == Some(value)
            }
        });
        if let Some(source) = source {
            out.push((&item.pointer, (*source).clone()));
        }
    }
    out
}
