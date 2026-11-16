//! Structural edits a save can make in place, keeping the host's style and
//! comments where a line patch would move things: a renamed key (which
//! [`renames`] tells, the links first) keeps its line (`udp.buffer = 3 # udp` → `udp_renamed.buffer = 3 # udp`), and a
//! scalar that became a container is written as an inline value where the
//! scalar was (`listen = 1 # port` → `listen = { inner = 1 } # port`).
//! Each is kept only if it reads back as the new document.

use super::super::item_sources::{ItemSource, ItemSources, SaveTold};
use super::super::language::{ConfigLanguage, Span};
use super::{byte_range, reads_as, through_long_quote};
use crate::editor::language::pointer_key;
use crate::editor::paths::FieldPath;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// A key renamed: the pointer of its key in the host's value, its old
/// name and its new one.
pub(in super::super) type Rename = (String, String, String);

/// `previous` (reading `old`) with the keys renamed where the host writes
/// them, when `new` is `old` with only keys renamed ([`renames`] tells
/// which, the links first): an inline table renamed with the keys inside
/// it stays inline.
pub(in super::super) fn patch_key_renames(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
    told: SaveTold<'_>,
) -> Option<String> {
    let renames = renames(old, new, told.sources());
    if renames.is_empty() || with_renames(old, &renames, true) != *new {
        return None;
    }
    let patched = renamed(language, previous, &renames)?;
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// The keys `new` renames from `old`, each where `old` holds it, a key
/// inside a renamed one too. A key the links tell is a rename (a key `new`
/// gains whose source, in the host's last text, is a key of the same map
/// it loses), whatever it holds; of the keys no link tells, a key lost and
/// one gained later in the map holding the same value, or the same value
/// but for keys renamed inside it, are a rename.
pub(in super::super) fn renames(
    old: &Value,
    new: &Value,
    sources: Option<&ItemSources>,
) -> Vec<Rename> {
    let mut out = Vec::new();
    collect_renames(
        old,
        new,
        (&mut String::new(), &mut String::new()),
        sources,
        &mut out,
    );
    out
}

fn collect_renames(
    old: &Value,
    new: &Value,
    (at, now): (&mut String, &mut String),
    sources: Option<&ItemSources>,
    out: &mut Vec<Rename>,
) {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            let lost: Vec<&String> = left
                .keys()
                .filter(|key| !right.contains_key(*key))
                .collect();
            let gained: Vec<&String> = right
                .keys()
                .filter(|key| !left.contains_key(*key))
                .collect();
            let mut to: HashMap<&String, &String> = HashMap::new();
            let mut taken: HashSet<&String> = HashSet::new();
            if let Some(sources) = sources {
                for key in &gained {
                    let Some(ItemSource::Previous(from)) =
                        sources.items.get(&format!("{now}/{}", pointer_key(key)))
                    else {
                        continue;
                    };
                    let Some((parent, name)) = from.rsplit_once('/') else {
                        continue;
                    };
                    if parent != at.as_str() {
                        continue;
                    }
                    if let Some(lost) = lost
                        .iter()
                        .find(|lost| pointer_key(lost) == name && !to.contains_key(*lost))
                    {
                        to.insert(lost, key);
                        taken.insert(key);
                    }
                }
            }
            let untold: Vec<&String> = lost
                .iter()
                .copied()
                .filter(|lost| !to.contains_key(lost))
                .collect();
            let mut next = 0;
            for lost in untold {
                let found = (next..gained.len()).find(|at| {
                    !taken.contains(gained[*at])
                        && same_but_renamed(&left[lost], &right[gained[*at]])
                });
                if let Some(at) = found {
                    next = at + 1;
                    to.insert(lost, gained[at]);
                    taken.insert(gained[at]);
                }
            }
            for (key, value) in left {
                let Some(name) = to
                    .get(key)
                    .copied()
                    .or_else(|| right.contains_key(key).then_some(key))
                else {
                    continue;
                };
                let (at_length, now_length) = (at.len(), now.len());
                at.push('/');
                at.push_str(&pointer_key(key));
                if name != key {
                    out.push((at.clone(), key.clone(), name.clone()));
                }
                now.push('/');
                now.push_str(&pointer_key(name));
                collect_renames(value, &right[name], (at, now), sources, out);
                at.truncate(at_length);
                now.truncate(now_length);
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (item, other)) in left.iter().zip(right).enumerate() {
                let (at_length, now_length) = (at.len(), now.len());
                at.push_str(&format!("/{index}"));
                now.push_str(&format!("/{index}"));
                collect_renames(item, other, (at, now), sources, out);
                at.truncate(at_length);
                now.truncate(now_length);
            }
        }
        _ => {}
    }
}

/// Whether `new` holds what `old` does, its keys renamed or not (same
/// values in the same places).
fn same_but_renamed(old: &Value, new: &Value) -> bool {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|((old_key, old_value), (new_key, new_value))| {
                        (old_key == new_key
                            || !left.contains_key(new_key) && !right.contains_key(old_key))
                            && same_but_renamed(old_value, new_value)
                    })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(old, new)| same_but_renamed(old, new))
        }
        _ => old == new,
    }
}

/// The pointer in the host's value of what `pointer` (in the value the
/// renames give) names: each renamed key on the way spelled as the host
/// held it.
pub(in super::super) fn source_pointer(renames: &[Rename], pointer: &str) -> String {
    if renames.is_empty() {
        return pointer.to_string();
    }
    let mut out = String::new();
    for segment in pointer.split('/').skip(1) {
        let renamed = renames.iter().find(|(at, _, new_key)| {
            pointer_key(new_key) == segment
                && at.rsplit_once('/').is_some_and(|(parent, _)| parent == out)
        });
        out.push('/');
        match renamed {
            Some((_, old_key, _)) => out.push_str(&pointer_key(old_key)),
            None => out.push_str(segment),
        }
    }
    out
}

/// `value` (read as `old` is when `forward`, else as the value the
/// renames give) with `renames` made, or undone when not `forward`.
pub(in super::super) fn with_renames(value: &Value, renames: &[Rename], forward: bool) -> Value {
    let mut by_parent: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for (pointer, old_key, new_key) in renames {
        let parent = pointer.rsplit_once('/').map_or("", |(parent, _)| parent);
        by_parent
            .entry(parent)
            .or_default()
            .push((old_key, new_key));
    }
    renamed_value(value, &by_parent, &mut String::new(), forward)
}

/// [`with_renames`] at `at` (the pointer of `value` in the host's value).
fn renamed_value(
    value: &Value,
    renames: &HashMap<&str, Vec<(&str, &str)>>,
    at: &mut String,
    forward: bool,
) -> Value {
    match value {
        Value::Object(map) => {
            let here = renames.get(at.as_str());
            Value::Object(
                map.iter()
                    .map(|(key, child)| {
                        let renamed = here.and_then(|here| {
                            here.iter().find_map(|(old_key, new_key)| {
                                let (from, to) = if forward {
                                    (old_key, new_key)
                                } else {
                                    (new_key, old_key)
                                };
                                (*from == key.as_str())
                                    .then(|| (to.to_string(), old_key.to_string()))
                            })
                        });
                        let (name, held) = renamed.unwrap_or_else(|| (key.clone(), key.clone()));
                        let length = at.len();
                        at.push('/');
                        at.push_str(&pointer_key(&held));
                        let child = renamed_value(child, renames, at, forward);
                        at.truncate(length);
                        (name, child)
                    })
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let length = at.len();
                    at.push_str(&format!("/{index}"));
                    let item = renamed_value(item, renames, at, forward);
                    at.truncate(length);
                    item
                })
                .collect(),
        ),
        scalar => scalar.clone(),
    }
}

/// `previous` with every rename made where the language writes the key
/// (its [`key_renames`](SaveLayout::key_renames), else the key's span),
/// all placed from one read of `previous` (one at a time, each read the
/// text so far again): each is where `previous` writes the key, by its
/// pointer there, a key inside a renamed one too. `None` when one cannot be made, or two
/// would write over each other.
fn renamed(
    language: &dyn ConfigLanguage,
    previous: &str,
    renames: &[(String, String, String)],
) -> Option<String> {
    let pointers: Vec<String> = renames
        .iter()
        .map(|(pointer, ..)| pointer.clone())
        .collect();
    let mut edits = super::super::paths::with_spans(language, previous, &pointers, || {
        let mut edits = Vec::with_capacity(renames.len());
        for (pointer, old_key, new_key) in renames {
            let written = language.written_key(previous, &FieldPath::from(pointer), new_key)?;
            match language.key_renames(previous, &FieldPath::from(pointer), &written) {
                Some(own) => edits.extend(own),
                None => edits.push(key_span_edit(
                    language, previous, pointer, old_key, &written,
                )?),
            }
        }
        Some(edits)
    })?;
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    edits.dedup();
    if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
        return None;
    }
    Some(super::super::language::with_edits(previous, edits))
}

/// The bytes of `source` a rename at the key's span replaces (quotes
/// around it stay), and their new text.
fn key_span_edit(
    language: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    old_key: &str,
    new_key: &str,
) -> Option<(std::ops::Range<usize>, String)> {
    let span = super::super::paths::memo_key_span(language, source, pointer)?;
    let range = byte_range(source, span)?;
    let written = &source[range.clone()];
    let flow = super::super::flow_syntax::Flow::of(language);
    let quote = written.chars().next().filter(|quote| {
        flow.quote(*quote).is_some()
            && written.len() >= 2 * quote.len_utf8()
            && written.ends_with(*quote)
    });
    let inner = match quote {
        Some(quote) => range.start + quote.len_utf8()..range.end - quote.len_utf8(),
        None => range.clone(),
    };
    if source[inner.clone()] != *old_key {
        return None;
    }
    let text = match quote {
        // A bare new key in the host's quotes.
        Some(quote) if !flow.starts_quoted(new_key) => format!("{quote}{new_key}{quote}"),
        _ => new_key.to_string(),
    };
    Some((range, text))
}

/// `previous` with a scalar that became a container written as an inline
/// value in its place, when that is the only change.
pub(in super::super) fn patch_scalar_to_container(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
    _told: SaveTold<'_>,
) -> Option<String> {
    let pointer = only_change(old, new, String::new())?;
    let value = new.pointer(&pointer)?;
    let was_scalar = old
        .pointer(&pointer)
        .is_some_and(|before| !before.is_object() && !before.is_array());
    if !was_scalar || !(value.is_object() || value.is_array()) {
        return None;
    }
    let text = language.format_value(value);
    if text.contains('\n') {
        return None;
    }
    let span: Span = language.value_span(previous, &FieldPath::from(&pointer))?;
    let mut range = byte_range(previous, span)?;
    range.end = through_long_quote(language, previous, range.clone());
    let mut patched = previous.to_string();
    patched.replace_range(range, &text);
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// The pointer of the one value `new` changes, keys and lengths kept.
fn only_change(old: &Value, new: &Value, pointer: String) -> Option<String> {
    if old == new {
        return None;
    }
    match (old, new) {
        (Value::Object(left), Value::Object(right))
            if left.len() == right.len() && left.keys().all(|key| right.contains_key(key)) =>
        {
            let mut differing = left.keys().filter(|key| left[*key] != right[*key]);
            let key = differing.next()?;
            if differing.next().is_some() {
                return None;
            }
            only_change(
                &left[key],
                &right[key],
                format!("{pointer}/{}", pointer_key(key)),
            )
        }
        _ => Some(pointer),
    }
}
