//! What a tab shows of its file's name. A file's own name no other tab has
//! shows with its folder (`conf.d/`), which a narrow strip leaves out; a
//! name several tabs share (`limits.toml` in three folders) shows after the
//! end of its folder that tells it apart (`eu-west-1/production/`), the
//! file's own name kept whole, and so do the first letters of that folder
//! that no other such tab starts with (`eu-…limits.toml` beside
//! `eu/limits.toml`): a narrow tab never cuts two alike. The title and
//! accessible name keep the whole path.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::editor::work_count::{Work, note, note_many};

/// What a tab shows of its file's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TabLabel {
    /// The folder a narrow strip leaves out.
    pub(super) folder: Option<String>,
    /// The start's first letters, which tell it from the other tabs'
    /// with the same file's name: never cut.
    pub(super) kept: String,
    /// What a narrow tab cuts (with an ellipsis).
    pub(super) start: String,
    /// What stays whole after it.
    pub(super) end: String,
}

/// The file's own name in `name`: past its last `/` or backslash.
fn base_name(name: &str) -> &str {
    name.rfind(['/', '\\']).map_or(name, |at| &name[at + 1..])
}

/// The folders of `folder` (`a/b/`), innermost first.
fn folders_inward(folder: &str) -> Vec<&str> {
    folder
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .filter(|_| !folder.is_empty())
        .collect()
}

/// How many characters a long name's end keeps whole.
const END_CHARS: usize = 12;

/// `name` as the start a narrow tab cuts (with an ellipsis) and the end it
/// keeps whole, so long names sharing a start still differ on screen
/// (`service-con…production.yaml`). A short name is all start.
pub(super) fn split_end(name: &str) -> (&str, &str) {
    if name.chars().count() <= END_CHARS + 4 {
        return (name, "");
    }
    let at = name
        .char_indices()
        .rev()
        .nth(END_CHARS - 1)
        .map_or(0, |(at, _)| at);
    name.split_at(at)
}

/// Every tab's label, by the file's own name its tab shares with others:
/// a tab's label depends only on the names it shares that name with, so a
/// list of the documents relabels only the groups whose names changed
/// ([`TabNames::relabel`]), each in one pass over its names (each tab
/// looking through every other name made a re-list of a thousand tabs
/// take seconds, QA 27 and 30).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TabNames {
    groups: HashMap<String, Arc<Group>>,
}

/// The labels of the tabs sharing a file's own name, by their whole name.
#[derive(Debug, PartialEq, Eq)]
struct Group {
    labels: HashMap<String, TabLabel>,
}

impl TabNames {
    /// The labels of the tabs of `names`, worked out afresh.
    #[cfg(test)]
    pub(super) fn new<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> Self {
        Self::default().relabel(names).unwrap_or_default()
    }

    /// The labels of the tabs of `names`, the groups whose names stay kept
    /// as they are; `None` when no group changed.
    pub(super) fn relabel<S: AsRef<str>>(
        &self,
        names: impl IntoIterator<Item = S>,
    ) -> Option<Self> {
        let names: Vec<S> = names.into_iter().collect();
        let mut by_base: HashMap<&str, HashSet<&str>> = HashMap::new();
        for name in &names {
            note(Work::TabNameVisit);
            let name = name.as_ref();
            by_base.entry(base_name(name)).or_default().insert(name);
        }
        let kept = |base: &str, alike: &HashSet<&str>| {
            self.groups.get(base).filter(|group| {
                group.labels.len() == alike.len()
                    && alike.iter().all(|name| group.labels.contains_key(*name))
            })
        };
        let same = by_base.len() == self.groups.len()
            && by_base
                .iter()
                .all(|(base, alike)| kept(base, alike).is_some());
        if same {
            return None;
        }
        let groups = by_base
            .iter()
            .map(|(base, alike)| {
                let group = kept(base, alike)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(Group::label(alike)));
                (base.to_string(), group)
            })
            .collect();
        Some(Self { groups })
    }
}

impl Group {
    /// The labels of `alike`, names sharing a file's own name.
    fn label(alike: &HashSet<&str>) -> Self {
        let names: Vec<&str> = alike.iter().copied().collect();
        note_many(Work::TabNameVisit, names.len());
        let base = names.first().map_or("", |name| base_name(name));
        if names.len() < 2 || base.is_empty() {
            let labels = names
                .iter()
                .map(|name| (name.to_string(), alone(name)))
                .collect();
            return Self { labels };
        }
        let starts: Vec<String> = telling_folders(&names)
            .iter()
            .map(|folder| format!("{folder}{}", shared_split(base).0))
            .collect();
        let kept = telling_letters(&starts);
        let labels = names
            .iter()
            .zip(&starts)
            .zip(kept)
            .map(|((name, start), kept)| {
                let (kept, start) = start.split_at(kept);
                let label = TabLabel {
                    folder: None,
                    kept: kept.to_string(),
                    start: start.to_string(),
                    end: shared_end(base).to_string(),
                };
                (name.to_string(), label)
            })
            .collect();
        Self { labels }
    }
}

/// What the tab of `name` shows, among the tabs of `names`.
pub(super) fn tab_label(name: &str, names: &TabNames) -> TabLabel {
    names
        .groups
        .get(base_name(name))
        .and_then(|group| group.labels.get(name))
        .cloned()
        .unwrap_or_else(|| alone(name))
}

/// The label of `name` when no other tab has its file's own name: the
/// name, its folder the part a narrow strip leaves out.
fn alone(name: &str) -> TabLabel {
    let base = base_name(name);
    let folder = &name[..name.len() - base.len()];
    let (start, end) = split_end(base);
    TabLabel {
        folder: (!folder.is_empty() && !base.is_empty()).then(|| folder.to_string()),
        kept: String::new(),
        start: if base.is_empty() { name } else { start }.to_string(),
        end: end.to_string(),
    }
}

/// The file's own name of a shared one, split: what a narrow tab may cut
/// (a long name's start) and what stays whole.
fn shared_split(base: &str) -> (&str, &str) {
    match split_end(base) {
        (whole, "") => ("", whole),
        split => split,
    }
}

/// What stays whole of a shared file's name: its end.
fn shared_end(base: &str) -> &str {
    shared_split(base).1
}

/// For each of `names`, all with the same file's own name, the end of its
/// folder that no other has (`eu-west-1/production/`): as few folders as
/// tell it apart, all of them when none do. One pass: how many names run
/// through each run of innermost folders, then each name's first run that
/// only it runs through.
fn telling_folders<'a>(names: &[&'a str]) -> Vec<&'a str> {
    let folders: Vec<&str> = names
        .iter()
        .map(|name| &name[..name.len() - base_name(name).len()])
        .collect();
    let inward: Vec<Vec<&str>> = folders
        .iter()
        .map(|folder| folders_inward(folder))
        .collect();
    let mut through: HashMap<&[&str], usize> = HashMap::new();
    for own in &inward {
        for depth in 1..=own.len() {
            *through.entry(&own[..depth]).or_default() += 1;
        }
    }
    folders
        .iter()
        .zip(&inward)
        .map(|(folder, own)| {
            let depth = (1..=own.len())
                .find(|&depth| through.get(&own[..depth]) == Some(&1))
                .unwrap_or(own.len());
            folder_end(folder, depth)
        })
        .collect()
}

/// The end of `folder` from the start of its `depth`-th folder counted
/// inward (all of it past the last; separators kept as written).
pub(super) fn folder_end(folder: &str, depth: usize) -> &str {
    if depth == 0 {
        return "";
    }
    let at = folder
        .trim_end_matches(['/', '\\'])
        .rmatch_indices(['/', '\\'])
        .nth(depth - 1)
        .map_or(0, |(at, _)| at + 1);
    &folder[at..]
}

/// For each of `starts`, how many bytes a narrow tab keeps whole so that
/// it never reads as another cut: one letter past the longest start it
/// shares with any other (all of it at most). In sorted order that
/// longest one is a neighbour.
fn telling_letters(starts: &[String]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..starts.len()).collect();
    order.sort_by(|a, b| starts[*a].cmp(&starts[*b]));
    let common = |a: &str, b: &str| a.chars().zip(b.chars()).take_while(|(a, b)| a == b).count();
    let mut shared = vec![0; starts.len()];
    for pair in order.windows(2) {
        let length = common(&starts[pair[0]], &starts[pair[1]]);
        shared[pair[0]] = shared[pair[0]].max(length);
        shared[pair[1]] = shared[pair[1]].max(length);
    }
    starts
        .iter()
        .zip(shared)
        .map(|(start, shared)| {
            start
                .char_indices()
                .nth(shared + 1)
                .map_or(start.len(), |(at, _)| at)
        })
        .collect()
}

#[cfg(test)]
#[path = "tab_label_tests.rs"]
mod tests;
