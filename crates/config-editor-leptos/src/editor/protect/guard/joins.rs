//! How many lines of an edited region would join each locked field once
//! the text parses, read for every lock at once: the region's lines are
//! read for their paths in one pass of the text, and each line is weighed
//! only against the locks its path relates it to (those it is inside, and
//! those inside the key it names), looked up by path. Weighing each line
//! against each lock, with a walk of the text for each path, froze a large
//! paste into a text with many locks for seconds (QA 33: 1,000 lines into
//! 300 host ranges, 10 s).

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use super::super::super::language::SchemaPathSegment;
use super::super::super::paths::FieldPath;
use super::super::super::work_count::{Work, note};
use super::super::locate::plain;
use super::super::scan::names_word;
use super::joining::{LinePaths, holds_code, names_holder};

/// One locked field, as the count reads it.
pub(super) struct Joinable<'p> {
    pub(super) pointer: &'p str,
    /// The lines near it (see `joining::near_lines`) in the text counted.
    pub(super) near: Range<usize>,
}

/// For each of `locks`, how many lines touching `region` of the text
/// `paths` reads would join it: lines whose path is inside it, and lines
/// naming a key that holds it (a second `web:` above a locked `web/port`).
/// Blank and comment lines join nothing. Only lines that could are
/// counted: those near the field and those naming one of its keys (a
/// duplicate, a TOML table or dotted key anywhere). With `own_key_free`, a
/// line naming the field's own key (a copy of its line, a duplicate the
/// parser may read last) is not counted.
pub(super) fn joining_counts(
    paths: &LinePaths<'_>,
    region: &Range<usize>,
    locks: &[Joinable<'_>],
    own_key_free: bool,
) -> Vec<usize> {
    let mut counts = vec![0; locks.len()];
    if locks.is_empty() {
        return counts;
    }
    let (behavior, lines) = (paths.behavior(), paths.lines());
    let first = lines.line_of(region.start);
    let last = lines.line_of(region.end);
    let keys: Vec<Vec<String>> = locks.iter().map(|lock| keys_of(lock.pointer)).collect();
    let words = Words::new(&keys);
    let near = near_any(locks, first, last);
    // The lines that could join a lock, their paths read at once.
    let candidates: Vec<usize> = (first..=last)
        .filter(|line| {
            let content = lines.line_text(*line);
            holds_code(behavior, content) && (near[line - first] || words.named(content))
        })
        .collect();
    paths.read_all(&candidates);
    let index = LockIndex::new(behavior, locks);
    for line in candidates {
        let Some(path) = paths.path(line) else {
            continue;
        };
        let content = lines.line_text(line);
        let reaches = |lock: usize| {
            note(Work::LockLine);
            locks[lock].near.contains(&line) || names_word(&keys[lock], content)
        };
        let path = plain(behavior, &path);
        // Inside the field (or the field itself): it joins it, unless it
        // is a line of its own key that may be.
        for lock in index.holding(&path) {
            let own_key = || {
                own_key_free
                    && keys[lock]
                        .last()
                        .is_some_and(|key| names_word(std::slice::from_ref(key), content))
            };
            if reaches(lock) && !own_key() {
                counts[lock] += 1;
            }
        }
        // A second key holding the field.
        if !path.is_empty() && names_holder(behavior, &path, content) {
            for &lock in index.inside(&path) {
                if reaches(lock) {
                    counts[lock] += 1;
                }
            }
        }
    }
    counts
}

/// The keys of the path `pointer`.
fn keys_of(pointer: &str) -> Vec<String> {
    FieldPath::from(pointer)
        .segments()
        .iter()
        .filter_map(|segment| match segment {
            SchemaPathSegment::Key(key) => Some(key.clone()),
            _ => None,
        })
        .collect()
}

/// For each line from `first` to `last`, whether it is near one of
/// `locks`: each lock's near lines marked at their ends, then summed.
fn near_any(locks: &[Joinable<'_>], first: usize, last: usize) -> Vec<bool> {
    let mut starts = vec![0isize; last - first + 2];
    for lock in locks {
        let from = lock.near.start.max(first);
        let to = lock.near.end.min(last + 1);
        if from < to {
            starts[from - first] += 1;
            starts[to - first] -= 1;
        }
    }
    let mut open = 0;
    starts[..=last - first]
        .iter()
        .map(|change| {
            open += change;
            open > 0
        })
        .collect()
}

/// Every key of the locks: those written as one word looked up among a
/// line's words, the others (holding a space or a dot) looked for in it.
struct Words<'k> {
    whole: HashSet<&'k str>,
    others: Vec<String>,
}

impl<'k> Words<'k> {
    fn new(keys: &'k [Vec<String>]) -> Self {
        let mut whole = HashSet::new();
        let mut others: Vec<String> = Vec::new();
        for key in keys.iter().flatten().filter(|key| !key.is_empty()) {
            if key.chars().all(is_word) {
                whole.insert(key.as_str());
            } else if !others.contains(key) {
                others.push(key.clone());
            }
        }
        Words { whole, others }
    }

    /// Whether `line` names one of the keys (`scan::names_word`).
    fn named(&self, line: &str) -> bool {
        line.split(|character: char| !is_word(character))
            .any(|word| self.whole.contains(word))
            || names_word(&self.others, line)
    }
}

/// A character of a word, as `scan::names_word` reads one.
fn is_word(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-')
}

/// The locks by path (in the plain form `locate::within` compares): each
/// one's own, and each path holding it.
struct LockIndex {
    own: HashMap<String, Vec<usize>>,
    holders: HashMap<String, Vec<usize>>,
}

impl LockIndex {
    fn new(
        behavior: &dyn super::super::super::language::ConfigLanguage,
        locks: &[Joinable<'_>],
    ) -> Self {
        let mut own: HashMap<String, Vec<usize>> = HashMap::new();
        let mut holders: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, lock) in locks.iter().enumerate() {
            let form = plain(behavior, lock.pointer);
            for (at, _) in form.match_indices('/').filter(|(at, _)| *at > 0) {
                holders
                    .entry(form[..at].to_string())
                    .or_default()
                    .push(index);
            }
            own.entry(form).or_default().push(index);
        }
        LockIndex { own, holders }
    }

    /// The locks `path` is, or is inside: those at `path` and at each path
    /// holding it, the whole document's included.
    fn holding<'s>(&'s self, path: &'s str) -> impl Iterator<Item = usize> + 's {
        let ends = path
            .match_indices('/')
            .map(|(at, _)| at)
            .chain([path.len()]);
        std::iter::once("")
            .chain(ends.filter(|at| *at > 0).map(|at| &path[..at]))
            .flat_map(|form| self.own.get(form).into_iter().flatten().copied())
    }

    /// The locks inside `path`, not at it.
    fn inside(&self, path: &str) -> &[usize] {
        self.holders.get(path).map_or(&[], Vec::as_slice)
    }
}
