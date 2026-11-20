//! What a view's links tell a save: each item of the view's text paired
//! with the links starting on its first line, and its source in the
//! host's last text or an earlier one; and links carried over a switch
//! to the items at the same pointers.

use super::super::super::item_sources::ItemSource;
use super::super::super::language::ConfigLanguage;
use super::super::super::parse_cache;
use super::super::super::text::TextSize;
use super::super::super::work_count::{Work, note, note_many};
use super::items::{Shared, items, items_and_shared, line_starts, shared_starts};
use super::{Holding, HostPaths, HostReading, Link, Links, Shown, Source, content, outer_first};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// What a save is told of the view's items: each one's source, by its
/// pointer in the view's value, and the host text before the last that
/// the [`ItemSource::Origin`] ones are in.
pub(crate) struct Told {
    pub(crate) items: HashMap<String, ItemSource>,
    /// The host text the [`ItemSource::Previous`] ones are in, when it is
    /// not the host's last text (no item comes from that): the save is
    /// an edit of it (an undo back past a save, a host emptied by a cut
    /// and its items pasted back).
    pub(crate) base: Option<String>,
    pub(crate) earlier: Option<String>,
    /// The links on lines with no item, kept across the save (see
    /// [`Links::saved`]).
    pub(crate) loose: Vec<Link>,
}

/// Items of a view by their pointers, each with its link's source.
type Paired = Vec<(String, Arc<Source>)>;

/// Whether `before` and `after` (in `language`) read as values with the
/// same keys in the same order and lists of the same lengths, all the
/// way down; `false` when either does not read.
pub(super) fn same_keys(language: &dyn ConfigLanguage, before: &str, after: &str) -> bool {
    fn shape(left: &serde_json::Value, right: &serde_json::Value) -> bool {
        use serde_json::Value;
        match (left, right) {
            (Value::Object(left), Value::Object(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|((key, left), (other, right))| key == other && shape(left, right))
            }
            (Value::Array(left), Value::Array(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| shape(left, right))
            }
            (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {
                false
            }
            _ => true,
        }
    }
    let (before, after) = (
        parse_cache::parsed(language, before),
        parse_cache::parsed(language, after),
    );
    match (before.as_ref(), after.as_ref()) {
        (Ok(before), Ok(after)) => shape(before, after),
        _ => false,
    }
}

/// Whether a link's host text is the host's last text. Each comparison
/// reads the whole text, and the links share a few host texts among all
/// their items: each is compared once.
struct LastHost<'a> {
    previous: &'a str,
    /// Each host text compared, by its address, and whether it is
    /// `previous`; the links hold the texts while this lives.
    compared: HashMap<*const u8, bool>,
}

impl<'a> LastHost<'a> {
    fn new(previous: &'a str) -> Self {
        LastHost {
            previous,
            compared: HashMap::new(),
        }
    }

    fn is(&mut self, host: &Arc<str>) -> bool {
        let previous = self.previous;
        *self
            .compared
            .entry(Arc::as_ptr(host).cast::<u8>())
            .or_insert_with(|| {
                if host.len() == previous.len() {
                    note_many(Work::HostTextCompared, host.len());
                }
                **host == *previous
            })
    }
}

impl Links {
    /// What the links tell a save of the view's text `text` (in
    /// `language`) over the host's last text `previous`: `None` when they
    /// tell nothing more than the values do (typing only, since a save
    /// whose host items hold no more than the view shows: `identity`
    /// false).
    pub(crate) fn told(
        &mut self,
        language: &dyn ConfigLanguage,
        text: &str,
        previous: &str,
        identity: bool,
    ) -> Option<Told> {
        // Typing only, its keys all where they were: each item is where
        // the patches find it by its values. Typing that renames a key or
        // a table's header moves every item under it.
        // Not when the links are of another host text than the last (an
        // undo back past a save): its items are no longer
        // where the last text has them.
        if let Links::Fresh { base, host, .. } = self
            && !identity
            && **host == *previous
            && same_keys(language, base, text)
        {
            return None;
        }
        let (paired, loose) = self.paired_and_loose(language, text)?;
        let mut last = LastHost::new(previous);
        // The host texts before the last the items come from, by how many
        // name each, most first.
        let mut named: Vec<(&Arc<str>, usize)> = Vec::new();
        let mut any_last = false;
        for (_, source) in &paired {
            if last.is(&source.host) {
                any_last = true;
                continue;
            }
            match named
                .iter_mut()
                .find(|(host, _)| Arc::ptr_eq(host, &source.host))
            {
                Some((_, count)) => *count += 1,
                None => named.push((&source.host, 1)),
            }
        }
        named.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let mut named = named.into_iter().map(|(host, _)| host.clone());
        // No item of the last text: the save is an edit of the text most
        // items come from.
        let base = (!any_last).then(|| named.next()).flatten();
        let earlier = named.next();
        let items = paired
            .into_iter()
            .filter_map(|(pointer, source)| {
                let of = |host: &Option<Arc<str>>| {
                    host.as_ref()
                        .is_some_and(|host| Arc::ptr_eq(host, &source.host))
                };
                let told = if of(&base) || (base.is_none() && last.is(&source.host)) {
                    ItemSource::Previous(source.pointer.clone())
                } else if of(&earlier) {
                    ItemSource::Origin(source.pointer.clone())
                } else {
                    return None;
                };
                Some((pointer, told))
            })
            .collect();
        Some(Told {
            items,
            base: base.map(|host| host.to_string()),
            earlier: earlier.map(|host| host.to_string()),
            loose: of_one_host(loose),
        })
    }

    /// The links once `text` (in `language`) is saved as the host text
    /// `host` (read as `reading`): each item of `text` the new host's item
    /// at its own pointer, as [`Links::fresh`] makes them, and the links of
    /// `text` on lines with no item kept with their sources (`loose`, as
    /// the save's [`Told`] read them, else read here). Those are an item's
    /// text pasted inside a scalar or a comment: cut and pasted as an item
    /// again, it is still that item, and keeps its form.
    pub(crate) fn saved(
        &mut self,
        language: &dyn ConfigLanguage,
        (host, text): (&str, &str),
        reading: Option<HostReading>,
        loose: Option<Vec<Link>>,
    ) -> Links {
        // None kept loose, and none read since: each is an item's.
        let none = matches!(self, Links::Fresh { loose, .. } if loose.is_empty());
        let kept = match loose {
            Some(loose) => loose,
            None if none => Vec::new(),
            None => self
                .paired_and_loose(language, text)
                .map(|(_, loose)| of_one_host(loose))
                .unwrap_or_default(),
        };
        let mut links = Links::fresh(host, text, reading);
        if let Links::Fresh { loose, .. } = &mut links {
            *loose = Arc::new(kept);
        }
        links
    }

    /// Each item of `text` (in `language`) with a link, and its source:
    /// paired with the links starting on its first line, outermost first;
    /// `None` when no item has one.
    pub(super) fn paired(&mut self, language: &dyn ConfigLanguage, text: &str) -> Option<Paired> {
        self.paired_and_loose(language, text)
            .map(|(paired, _)| paired)
    }

    /// [`Links::paired`], and the links starting on a line no item starts
    /// on.
    fn paired_and_loose(
        &mut self,
        language: &dyn ConfigLanguage,
        text: &str,
    ) -> Option<(Paired, Vec<Link>)> {
        if !self.fits(TextSize::of(text)) {
            return None;
        }
        let links = self.read(language);
        if links.is_empty() {
            return None;
        }
        let parsed = parse_cache::parsed(language, text);
        let value = parsed.as_ref().as_ref().ok()?;
        let starts = line_starts(text);
        let line_of = |at: TextSize| starts.partition_point(|start| *start <= at) - 1;
        let mut on_line: BTreeMap<usize, Vec<&Link>> = BTreeMap::new();
        for link in links.iter() {
            on_line
                .entry(line_of(link.range.start()))
                .or_default()
                .push(link);
        }
        // Each item's end, by the line it starts on.
        let mut items_on_line: BTreeMap<usize, Vec<(TextSize, String)>> = BTreeMap::new();
        let (found, shared) = items_and_shared(language, text, value);
        for item in found {
            note(Work::LinkStep);
            if let Some(range) = content(text, item.lines) {
                items_on_line
                    .entry(line_of(range.start()))
                    .or_default()
                    .push((range.end(), item.pointer));
            }
        }
        // The items lines do not tell apart, by the line they start on,
        // and where each starts (read only on a line holding links).
        let mut shared_on_line: BTreeMap<usize, Vec<Shared>> = BTreeMap::new();
        for item in shared {
            shared_on_line.entry(item.line).or_default().push(item);
        }
        let shared_at = |line: usize| {
            let shared = shared_on_line.get(&line)?;
            let line_start = *starts.get(line)?;
            let at = shared_starts(text, line_start, shared);
            Some(
                shared
                    .iter()
                    .map(|item| &item.pointer)
                    .zip(at)
                    .collect::<Vec<_>>(),
            )
        };
        let mut paired = Vec::new();
        let mut loose: Vec<Link> = Vec::new();
        for (line, links) in &on_line {
            if items_on_line.contains_key(line) {
                continue;
            }
            if let Some(shared) = shared_at(*line) {
                paired.extend(pair_shared(links, &mut vec![false; links.len()], &shared));
            }
            loose.extend(links.iter().map(|link| (*link).clone()));
        }
        let depth = |pointer: &str| pointer.matches('/').count();
        for (line, mut items) in items_on_line {
            let Some(links) = on_line.get_mut(&line) else {
                continue;
            };
            items.sort_by(|left, right| {
                right
                    .0
                    .cmp(&left.0)
                    .then(depth(&left.1).cmp(&depth(&right.1)))
            });
            links.sort_by(|left, right| {
                right
                    .range
                    .end()
                    .cmp(&left.range.end())
                    .then(depth(&left.source.pointer).cmp(&depth(&right.source.pointer)))
            });
            // A link ending where an item ends is that item's. Items
            // ending alike are each inside the one before (a TOML
            // `[a.b]` header writes `a` and `a/b`): their links pair the
            // innermost first, since an outer one with no text of its own
            // has no link a cut could have left. The rest pair by place.
            let mut item_used = vec![false; items.len()];
            let mut link_used = vec![false; links.len()];
            let name = |pointer: &str| pointer.rsplit('/').next().map(str::to_string);
            // An end two items share by chance (a pasted TOML header that
            // opens several tables, one ending where another's link ends)
            // is no pairing where the link's source names another item of
            // the line: that item is the link's (see the name pass below).
            let named_otherwise = |item: usize, link: &Link| {
                name(&items[item].1) != name(&link.source.pointer)
                    && items
                        .iter()
                        .any(|(_, pointer)| name(pointer) == name(&link.source.pointer))
            };
            for (index, (end, _)) in items.iter().enumerate() {
                if item_used[index] || items[..index].iter().any(|(other, _)| other == end) {
                    continue;
                }
                let alike: Vec<usize> = (index..items.len())
                    .filter(|at| items[*at].0 == *end)
                    .collect();
                let ending: Vec<usize> = (0..links.len())
                    .filter(|at| !link_used[*at] && links[*at].range.end() == *end)
                    .collect();
                for (item, link) in alike.iter().rev().zip(ending.iter().rev()) {
                    if named_otherwise(*item, links[*link]) {
                        continue;
                    }
                    paired.push((items[*item].1.clone(), links[*link].source.clone()));
                    item_used[*item] = true;
                    link_used[*link] = true;
                }
            }
            // Of the rest, a link names the item of its own name first (a
            // TOML `[a.b]` header left heading `a` once `[a]` is cut is
            // `b`'s, and a line moved into `b` moved its end), then they
            // pair by place.
            for (index, link) in links.iter().enumerate() {
                if link_used[index] {
                    continue;
                }
                let own = (0..items.len()).find(|at| {
                    !item_used[*at] && name(&items[*at].1) == name(&link.source.pointer)
                });
                if let Some(at) = own {
                    paired.push((items[at].1.clone(), link.source.clone()));
                    item_used[at] = true;
                    link_used[index] = true;
                }
            }
            // A link holding where an item sharing the line starts is that
            // item's (an entry joined onto its map's line), and is kept
            // across a save as a loose one is: the save's own links are
            // only of items with lines of their own.
            if let Some(shared) = shared_at(line) {
                let before = link_used.clone();
                paired.extend(pair_shared(links, &mut link_used, &shared));
                loose.extend(
                    links
                        .iter()
                        .zip(link_used.iter().zip(&before))
                        .filter(|(_, (now, was))| **now && !**was)
                        .map(|(link, _)| (*link).clone()),
                );
            }
            let left: Vec<&String> = items
                .iter()
                .zip(&item_used)
                .filter(|(_, used)| !**used)
                .map(|((_, pointer), _)| pointer)
                .collect();
            let unused = links
                .iter()
                .zip(&link_used)
                .filter(|(_, used)| !**used)
                .map(|(link, _)| link.source.clone());
            paired.extend(left.into_iter().cloned().zip(unused));
        }
        Some((paired, loose))
    }

    /// The links of `text` (in `language`), the text of these links
    /// (`from_text`, in `from_language`) converted: each item takes the
    /// source of the item at its pointer.
    pub(crate) fn converted(
        &mut self,
        from_language: &dyn ConfigLanguage,
        from_text: &str,
        language: &dyn ConfigLanguage,
        text: &str,
    ) -> Links {
        // The scalars the user left as the editor wrote them, which the
        // new text writes at the same pointers.
        let kept: Arc<super::KeptScalars> = Arc::new(
            self.untouched(from_language, from_text)
                .into_iter()
                .map(|untouched| (untouched.pointer, (untouched.host, untouched.from)))
                .collect(),
        );
        let sources: HashMap<String, Arc<Source>> = self
            .paired(from_language, from_text)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let parsed = parse_cache::parsed(language, text);
        let Ok(value) = parsed.as_ref() else {
            return Links::default();
        };
        // A view that writes a list of one as its item (XML) names it
        // without its index, or the other way: each item found where the
        // view it was written from holds it.
        let paths = match parse_cache::parsed(from_language, from_text).as_ref() {
            Ok(before) => HostPaths::between(before, value),
            Err(_) => HostPaths::default(),
        };
        // The switch wrote the new text: its items are shown there.
        let shown: Arc<str> = Arc::from(text);
        let mut links: Vec<Link> = items(language, text, value)
            .into_iter()
            .filter_map(|item| {
                let source = sources
                    .get(&item.pointer)
                    .or_else(|| sources.get(&paths.item(&item.pointer).0))?;
                let range = content(text, item.lines)?;
                let source = Arc::new(Source {
                    host: source.host.clone(),
                    pointer: source.pointer.clone(),
                    // A list of one the view before wrote as its item, which
                    // this one writes as a list.
                    holding: match source.holding {
                        Holding::ListShownAlone
                            if value
                                .pointer(&item.pointer)
                                .is_some_and(serde_json::Value::is_array) =>
                        {
                            Holding::AsShown
                        }
                        holding => holding,
                    },
                    shown: Some(Shown {
                        text: shown.clone(),
                        pointer: item.pointer,
                        language: language.info().label,
                        kept: Some(kept.clone()),
                    }),
                });
                Some(Link { range, source })
            })
            .collect();
        links.sort_by(outer_first);
        Links::Kept(Arc::new(links))
    }
}

/// Each of `links` not `used` paired with the first of `shared` (items
/// sharing the line, in document order, and where each starts) whose own
/// text starts inside it: the link of an item whose line was joined onto
/// another's still starts where that item's text does, and an outer item
/// comes before the ones inside it. Marks the links paired as used.
fn pair_shared(links: &[&Link], used: &mut [bool], shared: &[(&String, TextSize)]) -> Paired {
    let mut taken = vec![false; shared.len()];
    let mut paired = Vec::new();
    for (link, used) in links.iter().zip(used.iter_mut()) {
        if *used {
            continue;
        }
        let inside = (0..shared.len()).find(|at| !taken[*at] && link.range.contains(shared[*at].1));
        if let Some(at) = inside {
            paired.push((shared[at].0.clone(), link.source.clone()));
            taken[at] = true;
            *used = true;
        }
    }
    paired
}

/// Of `loose` links, those of the host text most of them name: a save
/// reads items from one earlier text (see [`Links::told`]), and each text
/// a link names stays in memory while it is kept.
fn of_one_host(loose: Vec<Link>) -> Vec<Link> {
    let mut named: Vec<(Arc<str>, usize)> = Vec::new();
    for link in &loose {
        match named
            .iter_mut()
            .find(|(host, _)| Arc::ptr_eq(host, &link.source.host))
        {
            Some((_, count)) => *count += 1,
            None => named.push((link.source.host.clone(), 1)),
        }
    }
    let Some((host, _)) = named.into_iter().max_by_key(|(_, count)| *count) else {
        return Vec::new();
    };
    loose
        .into_iter()
        .filter(|link| Arc::ptr_eq(&link.source.host, &host))
        .collect()
}
