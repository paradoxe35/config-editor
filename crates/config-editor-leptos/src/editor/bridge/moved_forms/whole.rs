//! A moved or copied item written back whole as its source wrote it: from
//! its key (or, for a list item, its value) through the end of its last
//! line, its comments and every form inside it included, its lines
//! indented where it now is. A save that wrote the item anew (its block,
//! table or element changed) keeps it as written (QA `FZ38-4`, `FZ38-11`),
//! where that text reads back the same there; else its scalars are
//! written back one by one (see `kept`).

use super::super::super::codec::{self, byte_range, written_range};
use super::super::super::language::ConfigLanguage;
use super::super::super::paths;
use super::{Moved, line_indent, reindented};
use serde_json::Value;
use std::ops::Range;

/// The bytes of the item at `pointer` in `text`: from its key's start (a
/// list item's: its value's) to the end of the line its value ends on.
fn item_range(host: &dyn ConfigLanguage, text: &str, pointer: &str) -> Option<Range<usize>> {
    let value = paths::memo_value_span(host, text, pointer)?;
    let value = written_range(host, text, value)?;
    let start = paths::memo_key_span(host, text, pointer)
        .and_then(|key| byte_range(text, key))
        .map_or(value.start, |key| key.start.min(value.start));
    let end = text[value.end..]
        .find('\n')
        .map_or(text.len(), |at| value.end + at);
    Some(start..end)
}

/// One at a time past this many writes is not tried (each try reads the
/// whole text).
const MOST_TRIED: usize = 16;

/// `saved` (a save over the host's last text `previous`, reading
/// `before`) with each of `moved` written whole as its source wrote it,
/// where the text then reads as `before` and the host finds no form
/// changed (an XML element's prefix still bound): all at once, else one
/// at a time.
pub(super) fn kept(
    host: &dyn ConfigLanguage,
    (saved, previous): (String, &str),
    before: &Value,
    moved: &[Moved<'_>],
    shape: Option<&Value>,
) -> String {
    // The write of `item` in `text`: its range there, and its source's
    // text indented where it is; `None` when it is written so already.
    let write = |text: &str, item: &Moved<'_>| -> Option<(Range<usize>, String)> {
        let now = item_range(host, text, &item.to)?;
        let was = item_range(host, item.source.text, &item.from)?;
        let written = &item.source.text[was.clone()];
        if text[now.clone()] == *written {
            return None;
        }
        let indent = line_indent(text, now.start);
        let from = line_indent(item.source.text, was.start);
        Some((now, reindented(written, from, indent)))
    };
    let inside = |inner: &str, outer: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    // No form changed the save did not change already.
    let changed = host.changed_form(previous, &saved);
    let reads = |text: &str| {
        codec::read(host, text, shape).ok().as_ref() == Some(before) && {
            let now = host.changed_form(previous, text);
            now.is_none() || now == changed
        }
    };
    // The outermost items (`moved` is in pointer order: an item before
    // the ones inside it), all at once.
    let mut outer: Vec<&Moved<'_>> = Vec::new();
    for item in moved {
        if !outer.last().is_some_and(|last| inside(&item.to, &last.to)) {
            outer.push(item);
        }
    }
    // Each text's spans read once (see `paths::with_spans`).
    let spans_of = |text: &str, from: bool| -> Vec<String> {
        moved
            .iter()
            .filter(|item| !from || std::ptr::eq(item.source.text, text))
            .map(|item| {
                if from {
                    item.from.clone()
                } else {
                    item.to.clone()
                }
            })
            .collect()
    };
    let batched = |text: &str, work: &dyn Fn() -> Vec<Option<(Range<usize>, String)>>| {
        let source = moved.first().map(|item| item.source.text);
        let (to, from) = (
            spans_of(text, false),
            source.map(|source| spans_of(source, true)),
        );
        paths::with_spans(host, text, &to, || match (source, from) {
            (Some(source), Some(from)) => paths::with_spans(host, source, &from, work),
            _ => work(),
        })
    };
    let all_writes = batched(&saved, &|| {
        moved.iter().map(|item| write(&saved, item)).collect()
    });
    let inside_outer = |item: &Moved<'_>| outer.iter().any(|outer| std::ptr::eq(*outer, item));
    let mut writes: Vec<(Range<usize>, String)> = moved
        .iter()
        .zip(&all_writes)
        .filter(|(item, _)| inside_outer(item))
        .filter_map(|(_, write)| write.clone())
        .collect();
    writes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let apart = writes
        .windows(2)
        .all(|pair| pair[1].0.end <= pair[0].0.start);
    if apart && !writes.is_empty() {
        let mut all = saved.clone();
        for (range, written) in &writes {
            all.replace_range(range.clone(), written);
        }
        if reads(&all) {
            return all;
        }
    }
    // Else one at a time, an outer item first: an item inside one
    // written is written with it.
    let differing: Vec<&Moved<'_>> = moved
        .iter()
        .zip(&all_writes)
        .filter(|(_, write)| write.is_some())
        .map(|(item, _)| item)
        .collect();
    if differing.len() > MOST_TRIED {
        return saved;
    }
    let mut text = saved;
    let mut written: Vec<&str> = Vec::new();
    for item in differing {
        if written.iter().any(|outer| inside(&item.to, outer)) {
            continue;
        }
        let Some((range, replaced)) = write(&text, item) else {
            continue;
        };
        let mut next = text.clone();
        next.replace_range(range, &replaced);
        if reads(&next) {
            text = next;
            written.push(&item.to);
        }
    }
    text
}
