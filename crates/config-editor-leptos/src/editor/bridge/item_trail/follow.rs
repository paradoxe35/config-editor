//! The items of a view followed through one edit: an item on lines it
//! writes back as they were is kept; an item whose first line it changes
//! is removed, kept as cut when it removes all its lines;
//! an item whose lines it inserts again, from the cut ones, is that item,
//! where it is inserted, and so is each item cut inside it, at the same
//! place in it.

use super::super::super::work_count::{Work, note};
use super::line_runs::{self, Changed};
use super::places::Places;
use super::{Cut, Followed, Splice};
use std::collections::{BTreeMap, HashMap};

/// `followed` (and the lines `cut` so far) through `splice`: of the
/// lines it spans, those it writes back as they were keep their items.
pub(super) fn follow(followed: &mut Vec<Followed>, cut: &mut Vec<Cut>, splice: &Splice) {
    let (at, end) = (splice.at, splice.at + splice.removed.len());
    let runs = line_runs::changed(&splice.removed, &splice.inserted);
    let grown = splice.inserted.len() as isize - splice.removed.len() as isize;
    let shift = |offset: usize, by: isize| (offset as isize + by) as usize;
    // Where the end of an item's lines (a line's start) is after the edit:
    // inside lines it changed, past what it inserted for them.
    let end_after = |offset: usize| {
        if offset <= at {
            return offset;
        }
        if offset > end {
            return shift(offset, grown);
        }
        let within = offset - at;
        match line_runs::holding(&runs, within) {
            Some(run) if within > run.removed.start => at + run.inserted.end,
            _ => at + shift(within, line_runs::moved_by(&runs, within, false)),
        }
    };
    for item in followed.iter_mut().filter(|item| item.alive) {
        let lines = item.lines.clone();
        if at <= lines.start && lines.start < end {
            let start = lines.start - at;
            if line_runs::holding(&runs, start).is_none() {
                item.lines = at + shift(start, line_runs::moved_by(&runs, start, true))
                    ..end_after(lines.end);
                continue;
            }
            item.alive = false;
            if lines.end <= end {
                let within = start..lines.end - at;
                cut.push(Cut {
                    lines: splice.removed[within.clone()].to_string(),
                    source: item.source.clone(),
                    key: item.key.clone(),
                    within: Some(within),
                });
            }
        } else if lines.start >= end {
            item.lines = shift(lines.start, grown)..shift(lines.end, grown);
        } else {
            item.lines.end = end_after(lines.end);
        }
    }
    let found = inserted_again(cut, &splice.inserted, &runs);
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
/// first place its lines start a line, in lines `runs` changed, not taken
/// by another.
fn inserted_again(cut: &[Cut], inserted: &str, runs: &[Changed]) -> Vec<Option<usize>> {
    let mut places = Places::of(inserted);
    // The lines written back as they were are the items kept there.
    let mut kept_from = 0;
    for run in runs {
        places.hold(kept_from..run.inserted.start);
        kept_from = run.inserted.end;
    }
    places.hold(kept_from..inserted.len());
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
