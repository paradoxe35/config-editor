//! The places of an inserted text where a cut item's lines may be written
//! again: the starts of its lines, found by what the lines from there
//! read, and the places items already took. A cut item is looked for
//! only where its lines are written, from where the last look for the
//! same lines stopped (QA `SP37-1`: each looked for over the whole insert,
//! each place weighed against every place taken, made the save of a large
//! edit cubic in its items).

use super::super::super::work_count::{Work, note};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

/// The step between two lines' hashes in the hash of a run of lines.
const STEP: u64 = 0x0000_0100_0000_01b3;

/// A line's hash (FNV-1a over its bytes, its line break included).
fn line_hash(line: &str) -> u64 {
    line.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(STEP)
    })
}

/// The hash of a run of lines from their hashes.
fn run_hash(lines: impl Iterator<Item = u64>) -> u64 {
    lines.fold(0, |hash, line| hash.wrapping_mul(STEP).wrapping_add(line))
}

pub(super) struct Places<'a> {
    text: &'a str,
    /// Where each line starts.
    starts: Vec<usize>,
    /// The hash of the lines before each line (and before the end).
    before: Vec<u64>,
    /// `STEP` to the power of each count of lines.
    powers: Vec<u64>,
    /// The lines starting each run of so many lines, by the run's hash.
    runs: HashMap<usize, HashMap<u64, Vec<usize>>>,
    /// The places taken, apart: start → end.
    taken: BTreeMap<usize, usize>,
}

impl<'a> Places<'a> {
    /// The places of `text`, none taken.
    pub(super) fn of(text: &'a str) -> Self {
        let mut starts = Vec::new();
        let mut before = vec![0u64];
        let mut at = 0;
        for line in text.split_inclusive('\n') {
            starts.push(at);
            at += line.len();
            let last = before[before.len() - 1];
            before.push(last.wrapping_mul(STEP).wrapping_add(line_hash(line)));
        }
        let mut powers = vec![1u64; starts.len() + 1];
        for count in 1..powers.len() {
            powers[count] = powers[count - 1].wrapping_mul(STEP);
        }
        Places {
            text,
            starts,
            before,
            powers,
            runs: HashMap::new(),
            taken: BTreeMap::new(),
        }
    }

    /// `place` taken, by lines no cut item is written on.
    pub(super) fn hold(&mut self, place: Range<usize>) {
        if !place.is_empty() {
            self.taken.insert(place.start, place.end);
        }
    }

    /// The first place at or after line `*from`'s start (of the places
    /// `lines` may start at) where `lines` are written and no place is
    /// taken, taken now; `*from` moved past every place passed, which no
    /// later look for the same `lines` can take.
    pub(super) fn take(&mut self, lines: &str, from: &mut usize) -> Option<usize> {
        if lines.is_empty() {
            return None;
        }
        // Its whole lines tell where it can start (a last line with no
        // break is matched from where they end).
        let whole: Vec<u64> = lines
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .map(line_hash)
            .collect();
        let count = whole.len();
        let candidates: Option<&[usize]> = if count == 0 {
            None
        } else {
            let hash = run_hash(whole.into_iter());
            let (starts, before, powers) = (&self.starts, &self.before, &self.powers);
            let runs = self.runs.entry(count).or_insert_with(|| {
                let mut runs: HashMap<u64, Vec<usize>> = HashMap::new();
                for line in 0..(starts.len() + 1).saturating_sub(count) {
                    let hash =
                        before[line + count].wrapping_sub(before[line].wrapping_mul(powers[count]));
                    runs.entry(hash).or_default().push(line);
                }
                runs
            });
            Some(runs.get(&hash).map_or(&[], Vec::as_slice))
        };
        let length = candidates.map_or(self.starts.len(), <[usize]>::len);
        while *from < length {
            let line = candidates.map_or(*from, |lines| lines[*from]);
            *from += 1;
            note(Work::TrailStep);
            let start = self.starts[line];
            let place = start..start + lines.len();
            if self.text[start..].starts_with(lines) && free(&self.taken, &place) {
                self.taken.insert(place.start, place.end);
                return Some(start);
            }
        }
        None
    }
}

/// Whether no place of `taken` (apart: start → end) overlaps `place`.
fn free(taken: &BTreeMap<usize, usize>, place: &Range<usize>) -> bool {
    taken
        .range(..place.end)
        .next_back()
        .is_none_or(|(_, end)| *end <= place.start)
}
