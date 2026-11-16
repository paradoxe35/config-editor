//! Which element of a host an element of a value written over it is: the
//! same key's element, and of repeated elements the item the edit made it
//! (`item_sources`: the lines the user kept, moved, cut and pasted), else
//! the item its values say (`places`). An item the edit added, or one of
//! two now where it had one copy, is no host element. A rewrite takes each
//! element's name, markup and layout from its own host element
//! (`attribute_layout`, `written_names`), and a save checks that it did
//! (`kept_markup`).

use super::Seg;
use crate::editor::item_sources::{ItemSource, ItemSources};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;

mod places;

pub(super) use places::alike;
use places::{Told, told_places};

/// Which host text an element is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Host {
    /// The host's last text.
    Previous,
    /// The text the edit session began on: an item cut in an earlier save
    /// and pasted back.
    Origin,
}

/// An edit of `old` (the host's value) to `new`.
pub(super) struct Identity<'a> {
    old: Option<&'a Value>,
    new: &'a Value,
    /// What the edit tells, and the value of the session's first text.
    told: Option<(&'a ItemSources, Option<&'a Value>)>,
    /// Each list's items' places in the host's, by the list's pointer in
    /// `new`.
    lists: RefCell<HashMap<String, Vec<Option<usize>>>>,
}

impl<'a> Identity<'a> {
    /// `old` is `None` when the host does not read: each element is the
    /// host's at its own path.
    pub(super) fn of(old: Option<&'a Value>, new: &'a Value) -> Self {
        Identity { old, new, told: None, lists: RefCell::default() }
    }

    /// The identity the edit tells (`sources`, with `origin` the value of
    /// the text the session began on), values telling the rest.
    pub(super) fn told(mut self, sources: Option<&'a ItemSources>, origin: Option<&'a Value>) -> Self {
        self.told = sources.map(|sources| (sources, origin));
        self
    }

    /// The host path (as the host's text writes it: an index only for a
    /// repeated element) of the element at `path` in a text written from
    /// `new`, and the text it is in, or `None` for an element the edit
    /// added.
    pub(super) fn host_path(&self, path: &[Seg]) -> Option<(Vec<String>, Host)> {
        self.resolve(path, false)
    }

    /// For an element the edit added as an item of a list, the host path
    /// of the item it copies: one of that list's host items holding the
    /// same data.
    pub(super) fn copied(&self, path: &[Seg]) -> Option<Vec<String>> {
        if !matches!(path.last(), Some(Seg::Index(_))) || self.host_path(path).is_some() {
            return None;
        }
        self.resolve(path, true).filter(|(_, host)| *host == Host::Previous).map(|(path, _)| path)
    }

    /// [`Self::host_path`], or with `copy` the host item a last item the
    /// edit added holds the same data as.
    fn resolve(&self, path: &[Seg], copy: bool) -> Option<(Vec<String>, Host)> {
        let Some(first) = self.old else {
            return Some((super::strings(path), Host::Previous));
        };
        // The host element the walk is in: `None` inside an element the
        // edit added, where only an element moved in, which its link
        // names, is a host element again.
        let mut old = Some(first);
        let mut new = self.new;
        let mut side = Host::Previous;
        // The list's pointer in `new` (an index for every item), and each
        // value's pointer as written (an index only in a list).
        let mut pointer = String::new();
        let (mut old_at, mut new_at) = (String::new(), String::new());
        let mut host = Vec::with_capacity(path.len());
        let mut at = 0;
        while at < path.len() {
            let Seg::Key(key) = &path[at] else { return None };
            let Value::Object(new_map) = new else { return None };
            let new_child = new_map.get(key)?;
            pointer.push('/');
            pointer.push_str(key);
            new_at.push('/');
            new_at.push_str(&escaped(key));
            let new_items = items(new_child);
            let index = match path.get(at + 1) {
                Some(Seg::Index(index)) => {
                    at += 1;
                    *index
                }
                _ if new_items.len() == 1 => 0,
                _ => return None,
            };
            let new_item = new_items.get(index)?;
            let item_at = |index: usize| match new_child {
                Value::Array(_) => format!("{new_at}/{index}"),
                _ => new_at.clone(),
            };
            // Pasted back from the session's first text: that item (or
            // key, which the host's last text may not hold); or moved from
            // another element of the host's last text, a descendant of one
            // too, also into an element pasted back or one the edit added:
            // that one.
            let added = old.is_none();
            let moved = self.told.and_then(|(sources, origin)| match sources.of(&item_at(index), !new_child.is_array())? {
                ItemSource::Origin(from) if added || side == Host::Previous || !of_parent(from, &old_at) => {
                    Some((origin?.pointer(from)?, from, Host::Origin))
                }
                ItemSource::Origin(_) => None,
                ItemSource::Previous(from) if added || side == Host::Origin || !of_parent(from, &old_at) => {
                    Some((self.old?.pointer(from)?, from, Host::Previous))
                }
                ItemSource::Previous(_) => None,
            });
            if let Some((held, from, held_in)) = moved {
                old = Some(held);
                new = new_item;
                host = from.split('/').skip(1).map(crate::editor::language::unescaped_key).collect();
                side = held_in;
                new_at = item_at(index);
                old_at = from.clone();
                pointer.push('/');
                pointer.push_str(&index.to_string());
                at += 1;
                continue;
            }
            // The host element this one is, if any.
            let held = 'held: {
                let Some(Value::Object(old_map)) = old else { break 'held None };
                let Some(old_key) = host_key(old_map, new_map, key) else { break 'held None };
                let old_child = &old_map[old_key];
                old_at.push('/');
                old_at.push_str(&escaped(old_key));
                let old_items = items(old_child);
                let mut lists = self.lists.borrow_mut();
                let places = lists.entry(pointer.clone()).or_insert_with(|| {
                    let told: Vec<Told> = match self.told {
                        Some((sources, _)) => (0..new_items.len())
                            .map(|index| match (sources.of(&item_at(index), !new_child.is_array()), side) {
                                (None, _) => Told::Nothing,
                                // An item of the text this element is in.
                                (Some(ItemSource::Previous(from)), Host::Previous)
                                | (Some(ItemSource::Origin(from)), Host::Origin) => {
                                    match place_in(from, &old_at, old_child.is_array()) {
                                        Some(place) => Told::At(place),
                                        // The key of a list written as a
                                        // lone item: which item, values
                                        // tell.
                                        None if *from == old_at => Told::Nothing,
                                        None => Told::Elsewhere,
                                    }
                                }
                                (Some(_), _) => Told::Elsewhere,
                            })
                            .collect(),
                        None => vec![Told::Nothing; new_items.len()],
                    };
                    told_places(old_items, new_items, &told)
                });
                let place = match places.get(index).copied().flatten() {
                    Some(place) => Some(place),
                    None if copy && at + 1 == path.len() => old_items.iter().position(|held| alike(held, new_item)),
                    None => None,
                };
                place.map(|place| (old_key, old_child.is_array(), &old_items[place], place))
            };
            match held {
                Some((old_key, repeated, item, place)) => {
                    host.push(old_key.clone());
                    if repeated {
                        host.push(place.to_string());
                        old_at.push('/');
                        old_at.push_str(&place.to_string());
                    }
                    old = Some(item);
                }
                // An element the edit added: its descendants are host
                // elements only where their links name one.
                None => old = None,
            }
            if new_child.is_array() {
                new_at.push('/');
                new_at.push_str(&index.to_string());
            }
            pointer.push('/');
            pointer.push_str(&index.to_string());
            new = new_item;
            at += 1;
        }
        old.map(|_| (host, side))
    }
}

/// Whether the host key or item at `pointer` is one of the element at
/// `parent`'s own: a key of it, or an item of a list of it.
fn of_parent(pointer: &str, parent: &str) -> bool {
    let Some(rest) = pointer.strip_prefix(parent).and_then(|rest| rest.strip_prefix('/')) else {
        return false;
    };
    match rest.split_once('/') {
        None => true,
        Some((_, index)) => crate::editor::language::pointer_index(index).is_some(),
    }
}

/// The place in the host list at `list` (holding a list when `repeated`,
/// else a lone item) of the host item at `pointer`, when it is one of its.
fn place_in(pointer: &str, list: &str, repeated: bool) -> Option<usize> {
    if !repeated {
        return (pointer == list || pointer.strip_prefix(list) == Some("/0")).then_some(0);
    }
    pointer.strip_prefix(list)?.strip_prefix('/')?.parse().ok()
}

fn escaped(key: &str) -> String {
    crate::editor::language::pointer_key(key)
}

/// The key of `old` that `new`'s `key` is: itself, or the one key `new`
/// renamed when it renames one.
pub(super) fn host_key<'m>(old: &'m Map<String, Value>, new: &Map<String, Value>, key: &str) -> Option<&'m String> {
    if let Some((held, _)) = old.get_key_value(key) {
        return Some(held);
    }
    let mut removed = old.keys().filter(|held| !new.contains_key(*held));
    let mut added = new.keys().filter(|added| !old.contains_key(*added));
    match (removed.next(), removed.next(), added.next(), added.next()) {
        (Some(held), None, Some(_), None) => Some(held),
        _ => None,
    }
}

/// A value as a list's items: a lone element is a list of one.
fn items(value: &Value) -> &[Value] {
    match value {
        Value::Array(items) => items,
        lone => std::slice::from_ref(lone),
    }
}
