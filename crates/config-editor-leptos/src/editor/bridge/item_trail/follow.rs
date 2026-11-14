//! The items of a view followed through one edit: an item whose first
//! line it removes is removed, kept as cut when it removes all its lines;
//! an item whose lines it inserts again, from the cut ones, is that item,
//! where it is inserted, and so is each item cut inside it, at the same
//! place in it.

use super::super::super::work_count::{Work, note};
use super::places::Places;
use super::{Cut, Followed, Splice};
use std::collections::{BTreeMap, HashMap};

/// `followed` (and the lines `cut` so far) through `splice`.
pub(super) fn follow(followed: &mut Vec<Followed>, cut: &mut Vec<Cut>, splice: &Splice) {
    let (at, end) = (splice.at, splice.at + splice.removed.len());
    let grown = splice.inserted.len() as isize - splice.removed.len() as isize;
    let shift = |offset: usize| (offset as isize + grown) as usize;
    for item in followed.iter_mut().filter(|item| item.alive) {
        let lines = item.lines.clone();
        if at <= lines.start && lines.start < end {
            item.alive = false;
            if lines.end <= end {
                let within = lines.start - at..lines.end - at;
                cut.push(Cut {
                    lines: splice.removed[within.clone()].to_string(),
                    source: item.source.clone(),
                    key: item.key.clone(),
                    within: Some(within),
                });
            }
        } else if lines.start >= end {
            item.lines = shift(lines.start)..shift(lines.end);
        } else if lines.end > end {
            item.lines.end = shift(lines.end);
        } else if lines.end > at {
            item.lines.end = at + splice.inserted.len();
        }
    }
    let found = inserted_again(cut, &splice.inserted);
    let mut kept = Vec::new();
    for (item, place) in std::mem::take(cut).into_iter().zip(found) {
        match place {
            Some(offset) => followed.push(Followed {
                lines: at + offset..at + offset + item.lines.len(),
                source: item.source,
                key: item.key,
                alive: true,
            }),
            None => kept.push(Cut {
                within: None,
                ..item
            }),
        }
    }
    *cut = kept;
}

/// Where in `inserted` each of `cut` is inserted again: an item cut
/// inside one inserted again is where it is in it; any other at the
/// first place its lines start a line, not taken by another.
fn inserted_again(cut: &[Cut], inserted: &str) -> Vec<Option<usize>> {
    let mut places = Places::of(inserted);
    // Where the last look for the same lines stopped.
    let mut passed: HashMap<&str, usize> = HashMap::new();
    // The items cut by this edit found by their lines, by where their
    // lines were in its removed text: start → (end, place inserted).
    let mut placed: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    cut.iter()
        .map(|item| {
            let inside = item.within.as_ref().and_then(|within| {
                note(Work::TrailStep);
                let (start, (end, place)) = placed.range(..=within.start).next_back()?;
                (within.end <= *end).then_some(place + within.start - start)
            });
            if inside.is_some() {
                return inside;
            }
            let from = passed.entry(item.lines.as_str()).or_default();
            let place = places.take(&item.lines, from)?;
            if let Some(within) = &item.within {
                placed.insert(within.start, (within.end, place));
            }
            Some(place)
        })
        .collect()
}
