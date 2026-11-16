//! A key or item an Edit-as edit moved or copied (cut or copied and
//! pasted, a line moved or copied, in one save or across a pause, a
//! switch, the host's language: its link tells which, see `links`) is
//! written in the form the host wrote it in where it was cut from, which
//! no view shows: an HCL expression or heredoc, a TOML datetime or hex
//! number, a YAML tag or quoting. Each of its scalars the save wrote
//! otherwise, reading the same, takes its written text back, and so does
//! each of its keys; a scalar that cannot (its text would read otherwise
//! there) makes the save wait, never pass written in another form. So do
//! the scalars left beside it in the map or list it landed in.

use super::super::codec::{self, span_text_range, written_text_range};
use super::super::item_sources::{ItemSource, SaveTold};
use super::super::language::{ConfigLanguage, Span, pointer_key};
use super::super::text::{TextRange, TextSize, slice};
use super::super::work_count::{Work, note};
use super::ItemMap;
use crate::editor::paths::FieldPath;

mod whole;
use serde_json::Value;
use std::collections::HashSet;

/// A host text an item was cut from, and its value.
#[derive(Clone, Copy)]
pub(super) struct Source<'a> {
    pub(super) text: &'a str,
    pub(super) value: &'a Value,
}

/// An item moved: its pointer in the save, and in the text it was cut
/// from.
struct Moved<'a> {
    to: String,
    from: String,
    source: Source<'a>,
}

/// A scalar or key of a moved item, by its pointer in the save and in
/// the text it was cut from.
struct Pair {
    to: String,
    from: String,
    /// Which moved item it is of.
    item: usize,
    /// A key (else a scalar value).
    key: bool,
}

/// A scalar or key of a moved item the save wrote otherwise than where
/// it was: its range in the save, and its text there.
struct Respelled<'a> {
    pointer: String,
    range: TextRange,
    written: &'a str,
    /// The indent of the line it starts on where it was.
    indent: &'a str,
    /// A value, which must be written as it was (a key is laid out).
    needed: bool,
}

/// The items `items` tells are somewhere else than where their source
/// was: pasted back from an earlier host text (`origin`), or from another
/// place of the host's last text (`previous`: another map or list, a map
/// renamed, a table its header no longer starts, another place in its
/// list, a copy). Where the patches kept an item's text, there is nothing
/// to write back; where they wrote it anew (its container changed), its
/// source's text is written back. Each, inside another one too, is
/// paired from its own source, not its parent's.
fn moved<'a>(items: &ItemMap, previous: Source<'a>, origin: Option<Source<'a>>) -> Vec<Moved<'a>> {
    let mut moved: Vec<Moved<'_>> = items
        .iter()
        .filter_map(|(to, source)| {
            let (from, source) = match source {
                ItemSource::Origin(from) => (from, origin?),
                ItemSource::Previous(from) if from != to => (from, previous),
                ItemSource::Previous(_) => return None,
            };
            Some(Moved {
                to: to.clone(),
                from: from.clone(),
                source,
            })
        })
        .collect();
    moved.sort_by(|left, right| left.to.cmp(&right.to));
    moved
}

/// The scalars under `to` in the save that hold what they held under
/// `from` in the text they were cut from, and the keys of the maps
/// there: pointers in each, by pairs.
fn pairs(
    saved: &Value,
    held: &Value,
    (to, from): (String, String),
    (item, own): (usize, &HashSet<String>),
    out: &mut Vec<Pair>,
) {
    match (saved, held) {
        (Value::Object(saved), Value::Object(held)) => {
            for (key, child) in saved {
                if let Some(was) = held.get(key) {
                    let key = pointer_key(key);
                    let (to, from) = (format!("{to}/{key}"), format!("{from}/{key}"));
                    // An item with a source of its own is paired from it.
                    if own.contains(&to) {
                        continue;
                    }
                    out.push(Pair {
                        to: to.clone(),
                        from: from.clone(),
                        item,
                        key: true,
                    });
                    pairs(child, was, (to, from), (item, own), out);
                }
            }
        }
        (Value::Array(saved), Value::Array(held)) => {
            for (index, (child, was)) in saved.iter().zip(held).enumerate() {
                let to = format!("{to}/{index}");
                if own.contains(&to) {
                    continue;
                }
                pairs(
                    child,
                    was,
                    (to, format!("{from}/{index}")),
                    (item, own),
                    out,
                );
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {}
        _ => {
            note(Work::MovedLeaf);
            if saved == held {
                out.push(Pair {
                    to,
                    from,
                    item,
                    key: false,
                });
            }
        }
    }
}

/// The text a scalar is written as, its lines told apart by what they
/// hold, not how deep they are indented.
fn unindented(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim_start)
}

/// The indent of the line of `text` holding byte `at`.
fn line_indent(text: &str, at: TextSize) -> &str {
    slice::after(text, slice::line_start_of(text, at)).map_or("", indent_of)
}

/// The blanks `line` starts with.
fn indent_of(line: &str) -> &str {
    let indent = TextSize::of(line) - TextSize::of(line.trim_start());
    slice::before(line, indent).unwrap_or_default()
}

/// `written` (a scalar's text over lines, its first line's indent
/// `from`) with its later lines indented from `to` as they were from
/// `from`.
fn reindented(written: &str, from: &str, to: &str) -> String {
    let mut lines = written.split('\n');
    let mut out = lines.next().unwrap_or_default().to_string();
    for line in lines {
        out.push('\n');
        if line.trim().is_empty() {
            out.push_str(line);
        } else {
            out.push_str(to);
            out.push_str(line.strip_prefix(from).unwrap_or(line));
        }
    }
    out
}

/// The spans of `pairs` (keys' or values') in the save and in their
/// source texts, each text read once.
fn spans(
    host: &dyn ConfigLanguage,
    saved: &str,
    pairs: &[&Pair],
    moved: &[Moved<'_>],
    key: bool,
) -> Vec<(Option<Span>, Option<Span>)> {
    // A member of a container written inline (`b: {sub: x}`) is found in
    // it.
    let lookup = |text: &str, pointers: &[String]| {
        let paths: Vec<FieldPath> = pointers.iter().map(FieldPath::from).collect();
        let spans = if key {
            host.key_spans(text, &paths)
        } else {
            host.value_spans(text, &paths)
        };
        spans
            .into_iter()
            .zip(pointers)
            .map(|(span, pointer)| {
                span.or_else(|| codec::inline_location(host, text, pointer, key))
            })
            .collect::<Vec<_>>()
    };
    let to: Vec<String> = pairs.iter().map(|pair| pair.to.clone()).collect();
    let now = lookup(saved, &to);
    let mut was = vec![None; pairs.len()];
    // The texts are the host's last and the session's first: told apart
    // by where they are, not compared.
    let same = |left: &str, right: &str| std::ptr::eq(left, right);
    let mut texts: Vec<&str> = Vec::new();
    for item in moved {
        if !texts.iter().any(|text| same(text, item.source.text)) {
            texts.push(item.source.text);
        }
    }
    for text in texts {
        let at: Vec<usize> = (0..pairs.len())
            .filter(|at| same(moved[pairs[*at].item].source.text, text))
            .collect();
        if at.is_empty() {
            continue;
        }
        let from: Vec<String> = at.iter().map(|at| pairs[*at].from.clone()).collect();
        for (at, span) in at.into_iter().zip(lookup(text, &from)) {
            was[at] = span;
        }
    }
    now.into_iter().zip(was).collect()
}

/// Each of `pairs` the save writes otherwise than its source text.
fn respelled<'a>(
    host: &dyn ConfigLanguage,
    saved: &str,
    pairs: &[Pair],
    moved: &[Moved<'a>],
) -> Vec<Respelled<'a>> {
    let mut out = Vec::new();
    for key in [false, true] {
        let pairs: Vec<&Pair> = pairs.iter().filter(|pair| pair.key == key).collect();
        if pairs.is_empty() {
            continue;
        }
        let range = |text: &str, span: Option<Span>| {
            let span = span?;
            if key {
                span_text_range(text, span)
            } else {
                written_text_range(host, text, span)
            }
        };
        for (pair, (now, was)) in pairs.iter().zip(spans(host, saved, &pairs, moved, key)) {
            let text = moved[pair.item].source.text;
            let (Some(now), Some(was)) = (range(saved, now), range(text, was)) else {
                continue;
            };
            let (Some(saved_text), Some(written)) =
                (slice::slice(saved, now), slice::slice(text, was))
            else {
                continue;
            };
            if unindented(saved_text).ne(unindented(written)) {
                out.push(Respelled {
                    pointer: pair.to.clone(),
                    range: now,
                    written,
                    indent: line_indent(text, was.start()),
                    needed: !key,
                });
            }
        }
    }
    out
}

/// `saved` (a save's text, reading `intended`) with the scalars and keys
/// of each item `items` (by their pointers in `intended`) tells the edit
/// moved written as where they were cut from (`previous`, or the
/// session's first text `origin`); `Err` with the pointer of a scalar
/// that cannot be, there.
pub(super) fn kept(
    host: &dyn ConfigLanguage,
    saved: String,
    intended: &Value,
    items: &ItemMap,
    (previous, origin): (Source<'_>, Option<Source<'_>>),
    shape: Option<&Value>,
    told: SaveTold<'_>,
) -> Result<String, String> {
    let saved = relocated(
        host,
        saved,
        intended,
        items,
        (previous, origin),
        shape,
        told,
    )?;
    // An item left where it was, holding what it held, that the save wrote
    // anew all the same (a list item's first entry removed: the next one
    // written onto the item's first line): written back as it was, where
    // that reads the same. Not one inside an item moved, which is that
    // item's.
    let moved = moved(items, previous, origin);
    let inside = |inner: &str| {
        moved.iter().any(|item| {
            inner
                .strip_prefix(item.to.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    };
    let mut unmoved: Vec<Moved<'_>> = items
        .iter()
        .filter_map(|(to, source)| match source {
            ItemSource::Previous(from)
                if from == to
                    && !inside(to)
                    && intended.pointer(to).is_some()
                    && intended.pointer(to) == previous.value.pointer(from) =>
            {
                Some(Moved {
                    to: to.clone(),
                    from: from.clone(),
                    source: previous,
                })
            }
            _ => None,
        })
        .collect();
    if unmoved.is_empty() {
        return Ok(saved);
    }
    unmoved.sort_by(|left, right| left.to.cmp(&right.to));
    let Some(before) = codec::read(host, &saved, shape).ok() else {
        return Ok(saved);
    };
    Ok(whole::kept(
        host,
        (saved, previous.text),
        &before,
        &unmoved,
        shape,
        told,
    ))
}

/// The scalars left where they were, holding what they held, in the maps
/// and lists `moved` items landed in (`own`: their pointers in the save),
/// each as an item of its own from its place in `previous`. An item moved
/// or copied in can make the patches write its new container anew (a
/// TOML table that gains a subtable, a list an item before the others):
/// the scalars beside it then take their host text back as its own do.
fn left_beside<'a>(
    items: &ItemMap,
    intended: &Value,
    previous: Source<'a>,
    moved: &[Moved<'a>],
    own: &HashSet<String>,
) -> Vec<Moved<'a>> {
    let mut parents: Vec<&str> = moved
        .iter()
        .filter_map(|item| item.to.rsplit_once('/').map(|(parent, _)| parent))
        .collect();
    parents.sort_unstable();
    parents.dedup();
    let mut beside = Vec::new();
    for parent in parents {
        // The container itself left where it was (linked there, or not
        // linked apart from its parent).
        let in_place = match items.get(parent) {
            Some(ItemSource::Previous(from)) => from == parent,
            Some(ItemSource::Origin(_)) => false,
            None => true,
        };
        let (true, Some(now), Some(was)) = (
            in_place,
            intended.pointer(parent),
            previous.value.pointer(parent),
        ) else {
            continue;
        };
        let children: Vec<(String, &Value)> = match now {
            Value::Object(map) => map
                .iter()
                .map(|(key, child)| (pointer_key(key), child))
                .collect(),
            Value::Array(list) => list
                .iter()
                .enumerate()
                .map(|(index, child)| (index.to_string(), child))
                .collect(),
            _ => continue,
        };
        for (step, child) in children {
            let to = format!("{parent}/{step}");
            let linked_here = match items.get(&to) {
                Some(ItemSource::Previous(from)) => *from == to,
                Some(ItemSource::Origin(_)) => false,
                None => true,
            };
            let scalar = !child.is_object() && !child.is_array();
            if !scalar || !linked_here || own.contains(&to) {
                continue;
            }
            // Compared once more, and counted, when paired.
            if was.pointer(&format!("/{step}")) == Some(child) {
                beside.push(Moved {
                    from: to.clone(),
                    to,
                    source: previous,
                });
            }
        }
    }
    beside
}

/// [`kept`] for the items somewhere else than their source.
fn relocated(
    host: &dyn ConfigLanguage,
    saved: String,
    intended: &Value,
    items: &ItemMap,
    (previous, origin): (Source<'_>, Option<Source<'_>>),
    shape: Option<&Value>,
    told: SaveTold<'_>,
) -> Result<String, String> {
    let moved = moved(items, previous, origin);
    if moved.is_empty() {
        return Ok(saved);
    }
    let read = |text: &str| codec::read(host, text, shape).ok();
    let Some(before) = read(&saved) else {
        return Ok(saved);
    };
    // Each item written whole as its source wrote it, where it reads the
    // same there; what is left, scalar by scalar.
    let saved = whole::kept(host, (saved, previous.text), &before, &moved, shape, told);
    let own: HashSet<String> = moved.iter().map(|item| item.to.clone()).collect();
    let mut beside = left_beside(items, intended, previous, &moved, &own);
    let mut moved = moved;
    moved.append(&mut beside);
    let mut found = Vec::new();
    for (index, item) in moved.iter().enumerate() {
        let (Some(now), Some(was)) = (
            intended.pointer(&item.to),
            item.source.value.pointer(&item.from),
        ) else {
            continue;
        };
        // Its own key, when it is a map's entry in both.
        let entry = |value: &Value, pointer: &str| {
            let (parent, _) = pointer.rsplit_once('/').unwrap_or_default();
            value.pointer(parent).is_some_and(Value::is_object)
        };
        let key = |pointer: &str| pointer.rsplit_once('/').map(|(_, key)| key.to_string());
        if entry(intended, &item.to)
            && entry(item.source.value, &item.from)
            && key(&item.to) == key(&item.from)
        {
            found.push(Pair {
                to: item.to.clone(),
                from: item.from.clone(),
                item: index,
                key: true,
            });
        }
        pairs(
            now,
            was,
            (item.to.clone(), item.from.clone()),
            (index, &own),
            &mut found,
        );
    }
    if found.is_empty() {
        return Ok(saved);
    }
    let mut respelled = respelled(host, &saved, &found, &moved);
    if respelled.is_empty() {
        return Ok(saved);
    }
    // Written back, the last first so the ranges before stay, as the text
    // reads unchanged: all at once, else one at a time.
    respelled.sort_by_key(|respelled| std::cmp::Reverse(respelled.range.start()));
    if respelled
        .windows(2)
        .any(|pair| pair[1].range.end() > pair[0].range.start())
    {
        respelled.retain(|respelled| respelled.needed);
    }
    let written = |text: &str, respelled: &Respelled<'_>, reindent: bool| {
        if reindent {
            let indent = line_indent(text, respelled.range.start());
            reindented(respelled.written, respelled.indent, indent)
        } else {
            respelled.written.to_string()
        }
    };
    let mut all = Some(saved.clone());
    for respelled in &respelled {
        all = all.and_then(|mut all| {
            let written = written(&all, respelled, true);
            slice::replace(&mut all, respelled.range, &written)?;
            Some(all)
        });
    }
    if let Some(all) = all
        && read(&all).as_ref() == Some(&before)
    {
        return Ok(all);
    }
    let mut text = saved;
    for respelled in &respelled {
        let kept = [true, false].into_iter().find_map(|reindent| {
            let mut next = text.clone();
            slice::replace(
                &mut next,
                respelled.range,
                &written(&text, respelled, reindent),
            )?;
            (read(&next).as_ref() == Some(&before)).then_some(next)
        });
        match kept {
            Some(next) => text = next,
            None if respelled.needed => return Err(respelled.pointer.clone()),
            None => {}
        }
    }
    Ok(text)
}
