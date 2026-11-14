//! A key or item an Edit-as edit moved (cut and pasted, in one save or
//! across a pause, a switch, the host's language: `item_trail` tells
//! which) is written in the form the host wrote it in where it was cut
//! from, which no view shows: an HCL expression or heredoc, a TOML
//! datetime or hex number, a YAML tag or quoting (QA `E36-1`). Each of
//! its scalars the save wrote otherwise, reading the same, takes its
//! written text back, and so does each of its keys; a scalar that cannot
//! (its text would read otherwise there) makes the save wait, never pass
//! written in another form.

use super::super::codec::{self, byte_range, written_range};
use super::super::item_sources::ItemSource;
use super::super::language::{ConfigLanguage, Span};
use super::super::work_count::{Work, note};
use super::ItemMap;
use serde_json::Value;
use std::ops::Range;

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
    range: Range<usize>,
    written: &'a str,
    /// The indent of the line it starts on where it was.
    indent: &'a str,
    /// A value, which must be written as it was (a key is laid out).
    needed: bool,
}

/// The items `items` tells were moved: pasted back from the session's
/// first text (`origin`), or from another map or list of the host's
/// last text (`previous`; in their own one, the patches keep their
/// lines). An item inside another moved is that one's.
fn moved<'a>(items: &ItemMap, previous: Source<'a>, origin: Option<Source<'a>>) -> Vec<Moved<'a>> {
    let parent = |pointer: &str| {
        pointer
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_string()
    };
    let mut moved: Vec<Moved<'_>> = items
        .iter()
        .filter_map(|(to, source)| {
            let (from, source) = match source {
                ItemSource::Origin(from) => (from, origin?),
                ItemSource::Previous(from) if parent(from) != parent(to) => (from, previous),
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
    let mut outer: Vec<Moved<'_>> = Vec::with_capacity(moved.len());
    for item in moved {
        let inside = outer.last().is_some_and(|last| {
            item.to
                .strip_prefix(&last.to)
                .is_some_and(|rest| rest.starts_with('/'))
        });
        if !inside {
            outer.push(item);
        }
    }
    outer
}

/// The scalars under `to` in the save that hold what they held under
/// `from` in the text they were cut from, and the keys of the maps
/// there: pointers in each, by pairs.
fn pairs(
    saved: &Value,
    held: &Value,
    (to, from): (String, String),
    item: usize,
    out: &mut Vec<Pair>,
) {
    match (saved, held) {
        (Value::Object(saved), Value::Object(held)) => {
            for (key, child) in saved {
                if let Some(was) = held.get(key) {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    let (to, from) = (format!("{to}/{key}"), format!("{from}/{key}"));
                    out.push(Pair {
                        to: to.clone(),
                        from: from.clone(),
                        item,
                        key: true,
                    });
                    pairs(child, was, (to, from), item, out);
                }
            }
        }
        (Value::Array(saved), Value::Array(held)) => {
            for (index, (child, was)) in saved.iter().zip(held).enumerate() {
                pairs(
                    child,
                    was,
                    (format!("{to}/{index}"), format!("{from}/{index}")),
                    item,
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
fn line_indent(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |line| line + 1);
    let line = &text[start..];
    &line[..line.len() - line.trim_start().len()]
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
        let spans = if key {
            host.key_spans(text, pointers)
        } else {
            host.value_spans(text, pointers)
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
                byte_range(text, span)
            } else {
                written_range(host, text, span)
            }
        };
        for (pair, (now, was)) in pairs.iter().zip(spans(host, saved, &pairs, moved, key)) {
            let text = moved[pair.item].source.text;
            let (Some(now), Some(was)) = (range(saved, now), range(text, was)) else {
                continue;
            };
            if unindented(&saved[now.clone()]).ne(unindented(&text[was.clone()])) {
                out.push(Respelled {
                    pointer: pair.to.clone(),
                    range: now,
                    written: &text[was.clone()],
                    indent: line_indent(text, was.start),
                    needed: !key,
                });
            }
        }
    }
    out
}

/// `saved` (a save's text, reading `intended`) with the scalars and keys
/// of each item `items` tells the edit moved written as where they were
/// cut from (`previous`, or the session's first text `origin`); `Err`
/// with the pointer of a scalar that cannot be, there.
pub(super) fn kept(
    host: &dyn ConfigLanguage,
    saved: String,
    intended: &Value,
    items: &ItemMap,
    (previous, origin): (Source<'_>, Option<Source<'_>>),
    shape: Option<&Value>,
) -> Result<String, String> {
    let moved = moved(items, previous, origin);
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
        if entry(intended, &item.to) && entry(item.source.value, &item.from) {
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
            index,
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
    respelled.sort_by_key(|respelled| std::cmp::Reverse(respelled.range.start));
    if respelled
        .windows(2)
        .any(|pair| pair[1].range.end > pair[0].range.start)
    {
        respelled.retain(|respelled| respelled.needed);
    }
    let read = |text: &str| codec::read(host, text, shape).ok();
    let Some(before) = read(&saved) else {
        return Ok(saved);
    };
    let written = |text: &str, respelled: &Respelled<'_>, reindent: bool| {
        if reindent {
            let indent = line_indent(text, respelled.range.start);
            reindented(respelled.written, respelled.indent, indent)
        } else {
            respelled.written.to_string()
        }
    };
    let mut all = saved.clone();
    for respelled in &respelled {
        let written = written(&all, respelled, true);
        all.replace_range(respelled.range.clone(), &written);
    }
    if read(&all).as_ref() == Some(&before) {
        return Ok(all);
    }
    let mut text = saved;
    for respelled in &respelled {
        let kept = [true, false].into_iter().find_map(|reindent| {
            let mut next = text.clone();
            next.replace_range(
                respelled.range.clone(),
                &written(&text, respelled, reindent),
            );
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
