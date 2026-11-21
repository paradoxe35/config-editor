//! Which host item each item of a value written over a host text is: the
//! one pairing every write and check of a save reads. The view's links
//! tell it ([`ItemSources`]: the items the user kept, moved, cut and
//! pasted); values tell only the items no link covers (an item added, a
//! text from outside the editor, a step written over a text an earlier
//! step wrote). A key is paired with the same key, or with the one key
//! its map renamed; a list item with the host item its link names, else
//! by its values ([`by_values`]). An item the edit added, or one of two now
//! where the host had one copy, is no host item.

use super::{ItemSource, ItemSources};
use crate::editor::codec::canonical;
use crate::editor::language::{pointer_index, pointer_key, unescaped_key};
use crate::editor::work_count::{Work, note};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

/// Which host text a host item is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// The host's last text.
    Previous,
    /// The earlier host text the links name ([`ItemSources::origin`]):
    /// an item cut in an earlier save and pasted back.
    Origin,
}

/// One step of a path into a written value: a key, and the item of the
/// list under it (`None` for a lone value, or for the list itself at the
/// path's end).
pub(crate) type Step = (String, Option<usize>);

/// An edit of `old` (the host's value) to `new`.
pub(crate) struct Pairing<'a> {
    old: Option<&'a Value>,
    new: &'a Value,
    /// What the links tell, and the value of the earlier host text.
    told: Option<(&'a ItemSources, Option<&'a Value>)>,
    /// Scalars compare by their text (a host that writes every scalar as
    /// text reads `<n>2</n>` as a number a view may hold as a string).
    texts: bool,
    /// Each list's items' places in the host's, by the list's pointer in
    /// `new` (an index for every item, a lone one's too).
    lists: RefCell<HashMap<String, Vec<Option<usize>>>>,
}

impl<'a> Pairing<'a> {
    /// `old` is `None` when the host does not read: each item is the
    /// host's at its own path.
    pub(crate) fn of(old: Option<&'a Value>, new: &'a Value, texts: bool) -> Self {
        Pairing {
            old,
            new,
            told: None,
            texts,
            lists: RefCell::default(),
        }
    }

    /// The pairing the links tell (`sources`, with `origin` the value of
    /// the earlier host text they name), values telling the rest.
    #[must_use]
    pub(crate) fn told(
        mut self,
        sources: Option<&'a ItemSources>,
        origin: Option<&'a Value>,
    ) -> Self {
        self.told = sources.map(|sources| (sources, origin));
        self
    }

    /// The host path (as the host's text writes it: an index only for a
    /// repeated item) of the item at `path` in `new`, and the text it is
    /// in, or `None` for an item the edit added.
    pub(crate) fn host_path(&self, path: &[Step]) -> Option<(Vec<String>, Side)> {
        self.resolve(path, false)
    }

    /// For an item the edit added to a list, the host path of the item it
    /// copies: one of that list's host items holding the same data.
    pub(crate) fn copied(&self, path: &[Step]) -> Option<Vec<String>> {
        if !matches!(path.last(), Some((_, Some(_)))) || self.host_path(path).is_some() {
            return None;
        }
        self.resolve(path, true)
            .filter(|(_, side)| *side == Side::Previous)
            .map(|(path, _)| path)
    }

    /// Which item of the host list at the same pointer each item of the
    /// list at `pointer` in `new` is, when that list is the host's list
    /// there (in the host's last text): its links first, values telling
    /// the rest. `None` when the list is not that host list.
    pub(crate) fn places(&self, pointer: &str) -> Option<Vec<Option<usize>>> {
        let path = steps(self.new, pointer)?;
        let ((_, None), parent) = path.split_last()? else {
            return None;
        };
        if !parent.is_empty() {
            let (host, side) = self.host_path(parent)?;
            let written: String = host
                .iter()
                .map(|key| format!("/{}", pointer_key(key)))
                .collect();
            if side != Side::Previous || written != pointer[..pointer.rfind('/')?] {
                return None;
            }
        }
        let old = self.old?.pointer(pointer)?.as_array()?;
        let new = self.new.pointer(pointer)?.as_array()?;
        let told = self.told_of(pointer, new.len(), (pointer, true), Side::Previous);
        Some(told_places(old, new, &told, self.texts))
    }

    /// [`Self::places`], else (the list is not that host list) by values
    /// alone, between the lists `old` and `new` of this pointer: each host
    /// item once (of an item and its copies, the first).
    pub(crate) fn list(&self, pointer: &str, old: &[Value], new: &[Value]) -> Vec<Option<usize>> {
        let mut places = self
            .places(pointer)
            .unwrap_or_else(|| by_values(old, new, self.texts));
        let mut taken = vec![false; old.len()];
        for place in &mut places {
            if let Some(at) = *place {
                if at >= old.len() || taken[at] {
                    *place = None;
                } else {
                    taken[at] = true;
                }
            }
        }
        places
    }

    /// What the links tell of each of the `count` items of the value at
    /// `new_at` in `new` (a list, else a lone item) in the host list at
    /// `old_at` (`repeated` when it is a list, else a lone item), in the
    /// text `side`.
    fn told_of(
        &self,
        new_at: &str,
        count: usize,
        (old_at, repeated): (&str, bool),
        side: Side,
    ) -> Vec<Told> {
        let Some((sources, _)) = self.told else {
            return vec![Told::Nothing; count];
        };
        let lone = self
            .new
            .pointer(new_at)
            .is_some_and(|value| !value.is_array());
        (0..count)
            .map(|index| {
                // Every item at its own pointer: the host's item there.
                let own = sources.in_place.then(|| {
                    ItemSource::Previous(if lone {
                        new_at.to_string()
                    } else {
                        format!("{new_at}/{index}")
                    })
                });
                let source = if own.is_some() {
                    own.as_ref()
                } else if lone {
                    sources.of(new_at, true)
                } else {
                    sources.items.get(&format!("{new_at}/{index}"))
                };
                match (source, side) {
                    (None, _) => Told::Nothing,
                    // An item of the text this list is in.
                    (Some(ItemSource::Previous(from)), Side::Previous)
                    | (Some(ItemSource::Origin(from)), Side::Origin) => {
                        match place_in(from, old_at, repeated) {
                            Some(place) => Told::At(place),
                            None => Told::Elsewhere,
                        }
                    }
                    (Some(_), _) => Told::Elsewhere,
                }
            })
            .collect()
    }

    /// [`Self::host_path`], or with `copy` the host item a last item the
    /// edit added holds the same data as.
    fn resolve(&self, path: &[Step], copy: bool) -> Option<(Vec<String>, Side)> {
        let Some(first) = self.old else {
            let mut host = Vec::with_capacity(path.len());
            for (key, index) in path {
                host.push(key.clone());
                host.extend(index.map(|index| index.to_string()));
            }
            return Some((host, Side::Previous));
        };
        // The host item the walk is in: `None` inside an item the edit
        // added, where only an item moved in, which its link names, is a
        // host item again.
        let mut old = Some(first);
        let mut new = self.new;
        let mut side = Side::Previous;
        // The list's pointer in `new` (an index for every item), and each
        // value's pointer as written (an index only in a list).
        let mut pointer = String::new();
        let (mut old_at, mut new_at) = (String::new(), String::new());
        let mut host = Vec::with_capacity(path.len());
        for (at, (key, index)) in path.iter().enumerate() {
            let Value::Object(new_map) = new else {
                return None;
            };
            let new_child = new_map.get(key)?;
            pointer.push('/');
            pointer.push_str(key);
            new_at.push('/');
            new_at.push_str(&pointer_key(key));
            let new_items = items(new_child);
            let index = match index {
                Some(index) => *index,
                None if new_items.len() == 1 => 0,
                None => return None,
            };
            let new_item = new_items.get(index)?;
            let item_at = match new_child {
                Value::Array(_) => format!("{new_at}/{index}"),
                _ => new_at.clone(),
            };
            // Pasted back from the earlier text: that item (or key, which
            // the host's last text may not hold); or moved from another
            // item of the host's last text, a descendant of one too, also
            // into an item pasted back or one the edit added: that one.
            let added = old.is_none();
            let moved = self.told.and_then(|(sources, origin)| {
                match sources.of(&item_at, !new_child.is_array())? {
                    ItemSource::Origin(from)
                        if added || side == Side::Previous || !of_parent(from, &old_at) =>
                    {
                        Some((origin?.pointer(from)?, from, Side::Origin))
                    }
                    ItemSource::Origin(_) => None,
                    ItemSource::Previous(from)
                        if added || side == Side::Origin || !of_parent(from, &old_at) =>
                    {
                        Some((self.old?.pointer(from)?, from, Side::Previous))
                    }
                    ItemSource::Previous(_) => None,
                }
            });
            if let Some((held, from, held_in)) = moved {
                old = Some(held);
                new = new_item;
                host = from.split('/').skip(1).map(unescaped_key).collect();
                side = held_in;
                new_at = item_at;
                old_at = from.clone();
                pointer.push('/');
                pointer.push_str(&index.to_string());
                continue;
            }
            // The host item this one is, if any.
            let held = 'held: {
                let Some(Value::Object(old_map)) = old else {
                    break 'held None;
                };
                let Some(old_key) = self.host_key(old_map, new_map, (key, &new_at), side) else {
                    break 'held None;
                };
                let old_child = &old_map[old_key];
                old_at.push('/');
                old_at.push_str(&pointer_key(old_key));
                let old_items = items(old_child);
                let mut lists = self.lists.borrow_mut();
                let places = lists.entry(pointer.clone()).or_insert_with(|| {
                    let told = self.told_of(
                        &new_at,
                        new_items.len(),
                        (&old_at, old_child.is_array()),
                        side,
                    );
                    in_gaps(
                        told_places(old_items, new_items, &told, self.texts),
                        old_items.len(),
                    )
                });
                let place = match places.get(index).copied().flatten() {
                    Some(place) => Some(place),
                    None if copy && at + 1 == path.len() => {
                        old_items.iter().position(|held| alike(held, new_item))
                    }
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
                // An item the edit added: its descendants are host items
                // only where their links name one.
                None => old = None,
            }
            if new_child.is_array() {
                new_at.push('/');
                new_at.push_str(&index.to_string());
            }
            pointer.push('/');
            pointer.push_str(&index.to_string());
            new = new_item;
        }
        old.map(|_| (host, side))
    }

    /// The key of `old` that `new`'s `key` (at `new_at` in `new`) is:
    /// itself; else the key its link names in this map (a rename the
    /// links tell); else, untold, the one key `new` renamed when it
    /// renames one.
    fn host_key<'m>(
        &self,
        old: &'m Map<String, Value>,
        new: &Map<String, Value>,
        (key, new_at): (&str, &str),
        side: Side,
    ) -> Option<&'m String> {
        if let Some((held, _)) = old.get_key_value(key) {
            return Some(held);
        }
        if let Some((sources, _)) = self.told
            && let Some(source) =
                sources.of(new_at, new.get(key).is_some_and(|child| !child.is_array()))
        {
            let in_side = matches!(
                (source, side),
                (ItemSource::Previous(_), Side::Previous) | (ItemSource::Origin(_), Side::Origin)
            );
            let named = source
                .pointer()
                .rsplit_once('/')
                .map(|(_, name)| unescaped_key(name));
            return named
                .filter(|_| in_side)
                .and_then(|name| old.get_key_value(&name))
                .map(|(held, _)| held)
                .filter(|held| !new.contains_key(*held));
        }
        renamed_from(old, new, key)
    }
}

/// The key of `old` that `new`'s `key` is where no link tells: itself, or
/// the one key `new` renamed when it renames one (a map losing one key
/// and gaining one).
pub(crate) fn renamed_from<'m>(
    old: &'m Map<String, Value>,
    new: &Map<String, Value>,
    key: &str,
) -> Option<&'m String> {
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

/// `pointer` (in `value`) as steps: each key, and the index under it of
/// the list it holds; `None` when it names no value, or a list in a list.
pub(crate) fn steps(value: &Value, pointer: &str) -> Option<Vec<Step>> {
    let mut segments = pointer.split('/').skip(1).peekable();
    let mut at = value;
    let mut out = Vec::new();
    while let Some(segment) = segments.next() {
        let key = unescaped_key(segment);
        let child = at.as_object()?.get(&key)?;
        match child {
            Value::Array(list) => match segments.peek().and_then(|next| pointer_index(next)) {
                Some(index) => {
                    segments.next();
                    at = list.get(index)?;
                    out.push((key, Some(index)));
                }
                None if segments.peek().is_none() => out.push((key, None)),
                None => return None,
            },
            _ => {
                at = child;
                out.push((key, None));
            }
        }
    }
    Some(out)
}

/// Whether the host key or item at `pointer` is one of the item at
/// `parent`'s own: a key of it, or an item of a list of it.
fn of_parent(pointer: &str, parent: &str) -> bool {
    let Some(rest) = pointer
        .strip_prefix(parent)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return false;
    };
    match rest.split_once('/') {
        None => true,
        Some((_, index)) => pointer_index(index).is_some(),
    }
}

/// The place in the host list at `list` (holding a list when `repeated`,
/// else a lone item) of the host item at `pointer`, when it is one of its.
fn place_in(pointer: &str, list: &str, repeated: bool) -> Option<usize> {
    if !repeated {
        return (pointer == list).then_some(0);
    }
    pointer_index(pointer.strip_prefix(list)?.strip_prefix('/')?)
}

/// A value as a list's items: a lone value is a list of one.
fn items(value: &Value) -> &[Value] {
    match value {
        Value::Array(items) => items,
        lone => std::slice::from_ref(lone),
    }
}

/// What the links tell of an item of the edited list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Told {
    /// Nothing: its values tell.
    Nothing,
    /// It is the host item at this place.
    At(usize),
    /// It is no item of this host list (added, or one of another list).
    Elsewhere,
}

/// Which item of `old` each item of `new` is: each item the links tell of
/// where they tell it (an item told twice is the host item and a copy of
/// it: both are it, so a copy keeps its form as a moved item does), the
/// rest by their values among the host items left.
fn told_places(old: &[Value], new: &[Value], told: &[Told], texts: bool) -> Vec<Option<usize>> {
    if told.iter().all(|told| *told == Told::Nothing) {
        return by_values(old, new, texts);
    }
    let mut placed = vec![None; new.len()];
    let mut taken = vec![false; old.len()];
    for (index, told) in told.iter().enumerate() {
        if let Told::At(place) = told
            && *place < old.len()
        {
            placed[index] = Some(*place);
            taken[*place] = true;
        }
    }
    let rest_new: Vec<usize> = (0..new.len())
        .filter(|index| told[*index] == Told::Nothing)
        .collect();
    let rest_old: Vec<usize> = (0..old.len()).filter(|place| !taken[*place]).collect();
    let values = |indices: &[usize], from: &[Value]| {
        indices
            .iter()
            .map(|index| from[*index].clone())
            .collect::<Vec<_>>()
    };
    let found = by_values(&values(&rest_old, old), &values(&rest_new, new), texts);
    for (at, place) in found.into_iter().enumerate() {
        placed[rest_new[at]] = place.map(|place| rest_old[place]);
    }
    placed
}

/// Which item of `old` each item of `new` is by their values, where no
/// link tells: the equal one in its place, else the first unused equal
/// one, else the unused one most like it (an item edited as it moved).
/// With `texts`, scalars are equal when their texts are ([`alike`]).
fn by_values(old: &[Value], new: &[Value], texts: bool) -> Vec<Option<usize>> {
    let same = |left: &Value, right: &Value| {
        if texts {
            alike(left, right)
        } else {
            left == right
        }
    };
    let key = |value: &Value| {
        if texts {
            canonical(&as_text(value))
        } else {
            canonical(value)
        }
    };
    let mut used = vec![false; old.len()];
    let mut found: Vec<Option<usize>> = new
        .iter()
        .enumerate()
        .map(|(slot, item)| {
            note(Work::ItemComparison);
            let stays = old.get(slot).is_some_and(|held| same(held, item));
            if stays {
                used[slot] = true;
            }
            stays.then_some(slot)
        })
        .collect();
    // The unused items of `old` by what they are, in order: each new item
    // finds its first unused equal one in a lookup.
    let mut equal: HashMap<String, VecDeque<usize>> = HashMap::new();
    if found.iter().any(Option::is_none) {
        for (at, item) in old.iter().enumerate().filter(|(at, _)| !used[*at]) {
            note(Work::ItemComparison);
            equal.entry(key(item)).or_default().push_back(at);
        }
    }
    for (slot, item) in new.iter().enumerate() {
        if found[slot].is_some() {
            continue;
        }
        note(Work::ItemComparison);
        if let Some(at) = equal.get_mut(&key(item)).and_then(VecDeque::pop_front) {
            used[at] = true;
            found[slot] = Some(at);
        }
    }
    // The likest pairs first, the nearest of equally like ones. Every
    // unmatched pair is compared up to a budget; past it (thousands of items
    // edited at once, a replace-all) each new item is compared with the
    // unused ones nearest its place, as many as the budget allows.
    let open: Vec<usize> = (0..new.len())
        .filter(|slot| found[*slot].is_none())
        .collect();
    let free: Vec<usize> = (0..old.len()).filter(|at| !used[*at]).collect();
    let reach = if open.len().saturating_mul(free.len()) <= PAIRS_COMPARED {
        free.len()
    } else {
        (PAIRS_COMPARED / open.len().max(1) / 2).max(1)
    };
    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    for &slot in &open {
        // The unused items around the same place.
        let middle = free.partition_point(|at| *at < slot);
        let from = middle.saturating_sub(reach);
        let to = (middle + reach).min(free.len());
        for &at in &free[from..to] {
            note(Work::ItemComparison);
            let score = likeness(&old[at], &new[slot], &same);
            if score > 0 {
                pairs.push((score, at, slot));
            }
        }
    }
    pairs.sort_by_key(|(score, at, slot)| (std::cmp::Reverse(*score), at.abs_diff(*slot), *slot));
    for (_, at, slot) in pairs {
        if !used[at] && found[slot].is_none() {
            used[at] = true;
            found[slot] = Some(at);
        }
    }
    found
}

/// `found` (each new item's place in a list of `count` old items) with
/// the items left between two placed in order, or before the first or
/// after the last, paired in order where as many old items as new ones are
/// left there: items changed in place. An element changed in place keeps
/// its own markup; a comment follows only an item its values or links tell
/// (see [`Pairing::list`]), so moves do not pair them.
fn in_gaps(mut found: Vec<Option<usize>>, count: usize) -> Vec<Option<usize>> {
    let mut used = vec![false; count];
    for place in found.iter().flatten() {
        if let Some(slot) = used.get_mut(*place) {
            *slot = true;
        }
    }
    let mut last_old = 0;
    let mut start_new = 0;
    for slot in 0..=found.len() {
        let place = if slot == found.len() {
            Some(used.len())
        } else {
            found[slot]
        };
        let Some(place) = place else { continue };
        if place < last_old {
            // Out of order: no gap around a moved item.
            start_new = slot + 1;
            continue;
        }
        let open_new: Vec<usize> = (start_new..slot)
            .filter(|at| found[*at].is_none())
            .collect();
        let open_old: Vec<usize> = (last_old..place).filter(|at| !used[*at]).collect();
        if !open_new.is_empty() && open_new.len() == open_old.len() {
            for (new, old) in open_new.into_iter().zip(open_old) {
                found[new] = Some(old);
                used[old] = true;
            }
        }
        last_old = place + 1;
        start_new = slot + 1;
    }
    found
}

/// The most pairs of unmatched items whose likeness one list's pairing
/// compares (see [`by_values`]).
const PAIRS_COMPARED: usize = 250_000;

/// What one scalar `new` keeps of `old` whole counts: more than any part.
const WHOLE: usize = 1000;

/// How much `new` still holds of `old`: each scalar a map (under the same
/// key) or a list keeps counts [`WHOLE`], a string edited the characters
/// its text keeps at its start and end. Zero when nothing ties them.
fn likeness(old: &Value, new: &Value, same: &dyn Fn(&Value, &Value) -> bool) -> usize {
    match (old, new) {
        _ if same(old, new) => WHOLE * leaves(old),
        (Value::Object(left), Value::Object(right)) => left
            .iter()
            .filter_map(|(key, value)| Some(likeness(value, right.get(key)?, same)))
            .sum(),
        (Value::Array(left), Value::Array(right)) => {
            left.iter()
                .filter(|item| right.iter().any(|other| same(item, other)))
                .map(leaves)
                .sum::<usize>()
                * WHOLE
        }
        (Value::String(left), Value::String(right)) => {
            let prefix = left
                .chars()
                .zip(right.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let shorter = left.chars().count().min(right.chars().count());
            let suffix = left
                .chars()
                .rev()
                .zip(right.chars().rev())
                .take_while(|(a, b)| a == b)
                .count()
                .min(shorter - prefix);
            (prefix + suffix).min(WHOLE - 1)
        }
        // A number or flag changed tells nothing of what it was.
        _ => 0,
    }
}

/// The scalars in `value` (at least one).
fn leaves(value: &Value) -> usize {
    match value {
        Value::Object(map) => map.values().map(leaves).sum::<usize>().max(1),
        Value::Array(items) => items.iter().map(leaves).sum::<usize>().max(1),
        _ => 1,
    }
}

/// A scalar's text, as a host that writes every scalar as text writes it.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

/// `value` with each scalar its text (see [`scalar_text`]).
fn as_text(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), as_text(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(as_text).collect()),
        scalar => Value::String(scalar_text(scalar).unwrap_or_default()),
    }
}

/// Whether two values hold the same data as a host that writes every
/// scalar as text writes it: scalars by their text, so a writer and a
/// save's check, reading the same items in other types, pair them alike.
fn alike(old: &Value, new: &Value) -> bool {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            old.len() == new.len()
                && new
                    .iter()
                    .all(|(key, value)| old.get(key).is_some_and(|held| alike(held, value)))
        }
        (Value::Array(old), Value::Array(new)) => {
            old.len() == new.len() && old.iter().zip(new).all(|(old, new)| alike(old, new))
        }
        _ => old == new || scalar_text(old).is_some_and(|old| Some(old) == scalar_text(new)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Told, by_values, told_places};
    use serde_json::{Value, json};

    fn items(values: &[i64]) -> Vec<Value> {
        values
            .iter()
            .map(|value| json!({"v": value, "k": value % 10}))
            .collect()
    }

    #[test]
    fn items_keep_their_places_wherever_the_edit_moves_them() {
        let old = items(&[1, 2, 3]);
        let cases: [(&[i64], &[Option<usize>]); 9] = [
            (&[2, 3], &[Some(1), Some(2)]),
            (&[1, 3], &[Some(0), Some(2)]),
            (&[0, 1, 2, 3], &[None, Some(0), Some(1), Some(2)]),
            (&[1, 2, 2, 3], &[Some(0), Some(1), None, Some(2)]),
            (&[2, 1, 3], &[Some(1), Some(0), Some(2)]),
            (&[3, 1, 2], &[Some(2), Some(0), Some(1)]),
            // Changed in place, all of them.
            (&[11, 12, 13], &[Some(0), Some(1), Some(2)]),
            // One removed, one added elsewhere: no pair.
            (&[1, 3, 9], &[Some(0), Some(2), None]),
            // One removed beside one changed: the one sharing a key.
            (&[1, 13], &[Some(0), Some(2)]),
        ];
        for texts in [false, true] {
            for (new, wanted) in cases {
                assert_eq!(by_values(&old, &items(new), texts), wanted, "{new:?}");
            }
        }
    }

    #[test]
    fn items_the_links_tell_of_keep_their_places_and_values_place_the_rest() {
        let old = items(&[1, 1, 3]);
        // The first of two equal items removed: the links say which.
        let new = items(&[1, 3]);
        assert_eq!(by_values(&old, &new, false), [Some(0), Some(2)]);
        assert_eq!(
            told_places(&old, &new, &[Told::At(1), Told::Nothing], false),
            [Some(1), Some(2)]
        );
        // Pasted back from elsewhere: no item of this list.
        assert_eq!(
            told_places(
                &old,
                &items(&[1, 1, 3]),
                &[Told::Elsewhere, Told::Nothing, Told::Nothing],
                false
            ),
            [None, Some(0), Some(2)]
        );
    }

    #[test]
    fn scalars_written_as_text_pair_by_their_text() {
        let old = vec![json!(2), json!("x")];
        assert_eq!(
            by_values(&old, &[json!("x"), json!("2")], true),
            [Some(1), Some(0)]
        );
        assert_eq!(
            by_values(&old, &[json!("x"), json!("2")], false),
            [Some(1), None]
        );
    }

    /// Pairing a list's items compares each a few times, however many:
    /// thousands of items edited at once (a replace-all in a view) would
    /// compare every unmatched pair, and look for each moved item among
    /// all the others.
    #[test]
    fn pairing_many_items_compares_each_a_few_times() {
        use crate::editor::work_count::{Work, counted};
        fn items(count: usize, text: &str) -> Vec<Value> {
            (0..count)
                .map(|index| json!(format!("{text} {index}")))
                .collect()
        }
        /// A list before and after an edit of `count` items.
        type Lists = fn(usize) -> (Vec<Value>, Vec<Value>);
        let cases: [(&str, Lists); 3] = [
            ("every item edited", |count| {
                (items(count, "value"), items(count, "REPLACED"))
            }),
            ("every item moved", |count| {
                let old = items(count, "value");
                let new = old.iter().rev().cloned().collect();
                (old, new)
            }),
            ("every item moved and edited", |count| {
                let old = items(count, "value");
                let new = items(count, "REPLACED").into_iter().rev().collect();
                (old, new)
            }),
        ];
        let mut failures = Vec::new();
        for (name, case) in cases {
            let compared = |count: usize| {
                let (old, new) = case(count);
                let (found, compared) =
                    counted(Work::ItemComparison, || by_values(&old, &new, false));
                assert_eq!(found.len(), count);
                compared
            };
            let (few, many) = (compared(1_000), compared(10_000));
            if many > 12 * few {
                failures.push(format!(
                    "{name}: {few} comparisons for 1,000 items, {many} for 10,000"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
