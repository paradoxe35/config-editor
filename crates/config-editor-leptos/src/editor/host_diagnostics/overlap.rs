//! The markers saying one message, found by where they are: a tree over
//! them by start, each branch knowing the furthest end of the markers in
//! it, so finding the ones a marker overlaps walks down to them instead of
//! comparing it with each (one host message on thousands of lines, every
//! problem compared with every other, after each pause in typing).

use std::collections::HashMap;
use std::hash::Hash;

use super::super::work_count::{Work, note};

/// A one-based `(line, column)` place.
pub(super) type Spot = (usize, usize);

/// Where a marker starts and where it ends (past its last character).
pub(super) type Span = (Spot, Spot);

/// No marker in a branch.
const NONE: usize = usize::MAX;

/// Markers by start, each in the tree once given an id ([`Self::add`]).
pub(super) struct Overlaps {
    /// Each marker's span, by start.
    spans: Vec<Span>,
    /// Leaves in the tree: the markers rounded up to a power of two.
    width: usize,
    /// For each branch (the root at 1, a branch's children at twice it and
    /// one more): the furthest end of the markers added in it.
    last_end: Vec<Option<Spot>>,
    /// For each branch: the lowest id of the markers added in it.
    first_id: Vec<usize>,
}

impl Overlaps {
    /// A tree for `spans` (by start), none added yet.
    pub(super) fn new(spans: Vec<Span>) -> Self {
        debug_assert!(spans.is_sorted_by_key(|&(start, _)| start));
        let width = spans.len().next_power_of_two();
        Self {
            spans,
            width,
            last_end: vec![None; 2 * width],
            first_id: vec![NONE; 2 * width],
        }
    }

    /// The marker at `rank` (its place in the spans by start) is in the
    /// tree, as `id`.
    pub(super) fn add(&mut self, rank: usize, id: usize) {
        let mut node = self.width + rank;
        self.last_end[node] = Some(self.spans[rank].1);
        self.first_id[node] = id;
        while node > 1 {
            node /= 2;
            self.last_end[node] = self.last_end[2 * node].max(self.last_end[2 * node + 1]);
            self.first_id[node] = self.first_id[2 * node].min(self.first_id[2 * node + 1]);
        }
    }

    /// How many markers start before `end`: those at a lower rank.
    fn before(&self, end: Spot) -> usize {
        self.spans.partition_point(|&(start, _)| start < end)
    }

    /// The id of every marker in the tree that `(start, end)` overlaps.
    pub(super) fn each(&self, (start, end): Span, mut found: impl FnMut(usize)) {
        let before = self.before(end);
        self.walk(1, 0, self.width, before, start, &mut found);
    }

    fn walk(
        &self,
        node: usize,
        low: usize,
        high: usize,
        before: usize,
        start: Spot,
        found: &mut impl FnMut(usize),
    ) {
        note(Work::MarkerComparison);
        if low >= before || self.last_end[node] <= Some(start) {
            return;
        }
        if node >= self.width {
            found(self.first_id[node]);
            return;
        }
        let middle = low.midpoint(high);
        self.walk(2 * node, low, middle, before, start, found);
        self.walk(2 * node + 1, middle, high, before, start, found);
    }

    /// The lowest id of the markers in the tree that `(start, end)`
    /// overlaps, where none of those in the tree overlap each other: the
    /// ones it overlaps are then those from the first that ends after its
    /// start to the last that starts before its end.
    pub(super) fn first_of_apart(&self, (start, end): Span) -> Option<usize> {
        let before = self.before(end);
        let first = self.first_ending_after(1, 0, self.width, before, start)?;
        Some(self.lowest_id(1, 0, self.width, first, before))
    }

    /// The lowest rank under `before` of a marker in the tree ending after
    /// `start`.
    fn first_ending_after(
        &self,
        node: usize,
        low: usize,
        high: usize,
        before: usize,
        start: Spot,
    ) -> Option<usize> {
        note(Work::MarkerComparison);
        if low >= before || self.last_end[node] <= Some(start) {
            return None;
        }
        if node >= self.width {
            return Some(low);
        }
        let middle = low.midpoint(high);
        self.first_ending_after(2 * node, low, middle, before, start)
            .or_else(|| self.first_ending_after(2 * node + 1, middle, high, before, start))
    }

    /// The lowest id of the markers in the tree ranked `from` to `to`.
    fn lowest_id(&self, node: usize, low: usize, high: usize, from: usize, to: usize) -> usize {
        note(Work::MarkerComparison);
        if high <= from || to <= low {
            return NONE;
        }
        if from <= low && high <= to {
            return self.first_id[node];
        }
        let middle = low.midpoint(high);
        self.lowest_id(2 * node, low, middle, from, to)
            .min(self.lowest_id(2 * node + 1, middle, high, from, to))
    }
}

/// The tree each problem's marker goes into, by what it says (`key`), and
/// its rank in it: `trees[tree]` for `placed[problem] == Some((tree, rank))`.
pub(super) fn by_key<K: Hash + Eq>(spans: &[Option<Span>], key: impl Fn(usize) -> K) -> Grouped<K> {
    let mut groups: HashMap<K, usize> = HashMap::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    for (problem, span) in spans.iter().enumerate() {
        if span.is_some() {
            let group = *groups.entry(key(problem)).or_insert_with(|| {
                members.push(Vec::new());
                members.len() - 1
            });
            members[group].push(problem);
        }
    }
    let mut placed = vec![None; spans.len()];
    let trees = members
        .into_iter()
        .enumerate()
        .map(|(tree, mut problems)| {
            problems.sort_by_key(|&problem| spans[problem]);
            for (rank, &problem) in problems.iter().enumerate() {
                placed[problem] = Some((tree, rank));
            }
            Overlaps::new(
                problems
                    .iter()
                    .filter_map(|&problem| spans[problem])
                    .collect(),
            )
        })
        .collect();
    Grouped {
        trees_by_key: groups,
        trees,
        placed,
    }
}

/// Problems' markers in a tree for each thing they say (see [`by_key`]).
pub(super) struct Grouped<K> {
    /// The tree of each key.
    pub(super) trees_by_key: HashMap<K, usize>,
    pub(super) trees: Vec<Overlaps>,
    /// Each problem's tree and rank in it (none without a marker).
    pub(super) placed: Vec<Option<(usize, usize)>>,
}
