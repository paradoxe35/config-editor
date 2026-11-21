//! A moved or copied item written back whole as its source wrote it: an
//! item on lines of its own as those lines (the comment lines right above
//! it, its key's or item's line, the lines its value runs on: its
//! comments and every form inside it included), indented where it now is,
//! the separator after it its place's; an item sharing its line from its
//! key (or, for a list item, its value) to its value's end. A save that
//! wrote the item anew (its block, table or element changed) keeps it as
//! written, where that text reads back the same there; else its scalars
//! are written back one by one (see `kept`).

use super::super::super::codec::{self, span_text_range, written_text_range};
use super::super::super::item_sources::SaveTold;
use super::super::super::language::{ConfigLanguage, pointer_key};
use super::super::super::paths;
use super::super::super::text::{TextRange, TextSize, slice, text_size};
use super::super::super::work_count::{Work, note_many};
use super::{Moved, indent_of, line_indent, reindented};
use crate::editor::paths::FieldPath;
use serde_json::Value;

/// A text split in lines once, for every item placed in it.
struct Lines<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
    /// Where each line starts.
    starts: Vec<TextSize>,
}

impl<'a> Lines<'a> {
    fn of(text: &'a str) -> Self {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = TextSize::default();
        for line in &lines {
            starts.push(at);
            at += TextSize::of(*line) + TextSize::from(1);
        }
        Lines {
            text,
            lines,
            starts,
        }
    }
}

/// Where an item is written.
enum Place {
    /// On lines of its own (see `codec::own_lines`: the comment lines
    /// right above it, its own, and those its value runs on): from the
    /// first one's first character that is not blank to the end of the
    /// last.
    Lines(TextRange),
    /// Sharing its line (in a container written on one line): from its
    /// key's start (a list item's: its value's) to its value's end.
    Inline(TextRange),
}

/// Where the item at `pointer` is written in `text`.
fn place(host: &dyn ConfigLanguage, text: &Lines<'_>, pointer: &str) -> Option<Place> {
    if let Some(lines) = codec::own_lines(host, text.text, &text.lines, pointer) {
        let first = text.lines[lines.start];
        let start = text.starts[lines.start] + TextSize::of(indent_of(first));
        let last = lines.end - 1;
        let end = text.starts[last] + TextSize::of(text.lines[last]);
        return Some(Place::Lines(TextRange::new(start, end.max(start))));
    }
    inline(host, text, pointer).map(Place::Inline)
}

/// The bytes of the item at `pointer` in `text` from its key's start (a
/// list item's: its value's) to its value's end.
fn inline(host: &dyn ConfigLanguage, text: &Lines<'_>, pointer: &str) -> Option<TextRange> {
    // A member of a container written inline is found in it, its key
    // with its quotes.
    let value = codec::inline_location(host, text.text, pointer, false)
        .or_else(|| paths::memo_value_span(host, text.text, pointer))?;
    let value = written_text_range(host, text.text, value)?;
    let start = codec::inline_location(host, text.text, pointer, true)
        .or_else(|| paths::memo_key_span(host, text.text, pointer))
        .and_then(|key| span_text_range(text.text, key))
        .map_or(value.start(), |key| key.start().min(value.start()));
    Some(TextRange::new(start, value.end()))
}

/// The write of the item `item` at its place in `text` as its source
/// (in `source`) writes it: its range in `text`, and the source's text
/// there, its lines indented where it is and the separator after it
/// (JSON's `,`) the place's; `None` when it is written so already, or
/// the two places differ in kind (lines of its own and a place in a
/// line).
fn write(
    host: &dyn ConfigLanguage,
    text: &Lines<'_>,
    source: &Lines<'_>,
    item: &Moved<'_>,
) -> Option<(TextRange, String)> {
    // The source's text from `was`, its key written as the place's (a
    // key renamed in the view keeps its new name).
    let renamed = |was: TextRange, now: TextRange| -> Option<String> {
        let key = |text: &str, pointer: &str| {
            paths::memo_key_span(host, text, pointer).and_then(|key| span_text_range(text, key))
        };
        let mut written = slice::slice(source.text, was)?.to_string();
        if let (Some(old), Some(new)) = (key(source.text, &item.from), key(text.text, &item.to)) {
            if !was.contains_range(old) || !now.contains_range(new) {
                return None;
            }
            slice::replace(
                &mut written,
                old - was.start(),
                slice::slice(text.text, new)?,
            )?;
        }
        Some(written)
    };
    let (now, written) = match (
        place(host, text, &item.to)?,
        place(host, source, &item.from)?,
    ) {
        (Place::Lines(now), Place::Lines(was)) => {
            let written = renamed(was, now)?;
            let written = written.as_str();
            let place_last = slice::slice(text.text, now)?
                .rsplit('\n')
                .next()
                .unwrap_or_default();
            let (head, last) = match written.rsplit_once('\n') {
                Some((head, last)) => (Some(head), last),
                None => (None, written),
            };
            let last = codec::with_comma_of(host, last, place_last);
            let written = match head {
                Some(head) => format!("{head}\n{last}"),
                None => last,
            };
            let indent = line_indent(text.text, now.start());
            let from = line_indent(source.text, was.start());
            let written = reindented(&written, from, indent);
            // The comment lines the place has above the item that its
            // source has not (another item's, which the save put there)
            // stay.
            let above = |lines: &Lines<'_>, start: TextSize, pointer: &str| -> Vec<String> {
                let top = lines.starts.partition_point(|at| *at <= start) - 1;
                first_line(host, lines.text, pointer)
                    .filter(|first| *first >= top)
                    .map(|first| {
                        lines.lines[top..first]
                            .iter()
                            .map(|line| line.trim().to_string())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let theirs = above(source, was.start(), &item.from);
            let others: Vec<String> = above(text, now.start(), &item.to)
                .into_iter()
                .filter(|line| !theirs.contains(line))
                .collect();
            let written = if others.is_empty() {
                written
            } else {
                format!("{}\n{indent}{written}", others.join(&format!("\n{indent}")))
            };
            (now, written)
        }
        (Place::Inline(now), Place::Inline(was)) => {
            let written = renamed(was, now)?;
            (now, written)
        }
        // An item its source writes on a line of its own, now sharing
        // its line (the save wrote it after a list item's marker): from
        // its key to its value's end, a value on one line only.
        (Place::Inline(now), Place::Lines(_)) => {
            let was = inline(host, source, &item.from)?;
            // Its value the save wrote on the lines below (a list after
            // `- args:`): replaced to its last scalar's end.
            let now = item.source.value.pointer(&item.from).map_or(now, |value| {
                through_value(host, text.text, &item.to, now, value)
            });
            let written = renamed(was, now)?;
            if written.contains('\n') {
                return None;
            }
            (now, written)
        }
        // An item its source writes sharing its line (in a map written
        // inline), now on lines of its own (the save wrote its new
        // container over lines): its source's text from its key to its
        // value's end, over the same part of its place.
        (Place::Lines(_), Place::Inline(was)) => {
            let now = inline(host, text, &item.to)?;
            let written = renamed(was, now)?;
            (now, written)
        }
    };
    (slice::slice(text.text, now)? != written).then_some((now, written))
}

/// `range` (the item at `pointer` of `text`, from its key) through the end
/// of every scalar its value `value` holds as `text` writes them: a value
/// written on the lines below the key it shares a line with. A value
/// whose last line closes a bracket after its last scalar is not reached:
/// the write then reads back otherwise, and is not kept.
fn through_value(
    host: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    range: TextRange,
    value: &Value,
) -> TextRange {
    fn leaves(value: &Value, pointer: &mut String, out: &mut Vec<String>) {
        let length = pointer.len();
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    pointer.push('/');
                    pointer.push_str(&pointer_key(key));
                    leaves(child, pointer, out);
                    pointer.truncate(length);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    pointer.push('/');
                    pointer.push_str(&index.to_string());
                    leaves(child, pointer, out);
                    pointer.truncate(length);
                }
            }
            _ => out.push(pointer.clone()),
        }
    }
    if !(value.is_object() || value.is_array()) {
        return range;
    }
    let mut inner = Vec::new();
    leaves(value, &mut pointer.to_string(), &mut inner);
    let end = inner
        .iter()
        .filter_map(|inner| {
            let span = paths::memo_value_span(host, text, inner)
                .or_else(|| codec::inline_location(host, text, inner, false))?;
            written_text_range(host, text, span)
        })
        .map(TextRange::end)
        .fold(range.end(), TextSize::max);
    TextRange::new(range.start(), end)
}

/// The line (from 0) the item at `pointer` of `text` starts on: its key's,
/// else its value's.
fn first_line(host: &dyn ConfigLanguage, text: &str, pointer: &str) -> Option<usize> {
    paths::memo_key_span(host, text, pointer)
        .or_else(|| paths::memo_value_span(host, text, pointer))?
        .line
        .checked_sub(1)
}

/// The comments of the item `item` (on lines of its own in both texts)
/// as its source writes them, for an item not written back whole (its
/// value changed since): the comment lines right above its first line,
/// and the comment after code on that line. The write over `text`: the
/// lines from its first comment line through its first line; `None` when
/// they are written so already, or a comment is somewhere else in the
/// text already.
fn comments(
    host: &dyn ConfigLanguage,
    text: &Lines<'_>,
    source: &Lines<'_>,
    item: &Moved<'_>,
) -> Option<(TextRange, String)> {
    let now = codec::own_lines(host, text.text, &text.lines, &item.to)?;
    let was = codec::own_lines(host, source.text, &source.lines, &item.from)?;
    let (first, from) = (
        first_line(host, text.text, &item.to)?,
        first_line(host, source.text, &item.from)?,
    );
    if !(now.contains(&first) && was.contains(&from)) {
        return None;
    }
    // The language's hook gives a `usize` byte of the line.
    let trailing = |line: &'_ str| -> Option<String> {
        host.comment_start(line)
            .and_then(|at| slice::after(line, text_size(at)))
            .map(|comment| comment.trim().to_string())
            .filter(|comment| !comment.is_empty())
    };
    let above: Vec<&str> = source.lines[was.start..from]
        .iter()
        .map(|line| line.trim())
        .collect();
    let line = text.lines[first];
    let indent = indent_of(line);
    let written: Vec<&str> = text.lines[now.start..first]
        .iter()
        .map(|line| line.trim())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut changed = false;
    if above != written && !above.is_empty() {
        if above.iter().any(|comment| text.text.contains(comment)) {
            return None;
        }
        out.extend(above.iter().map(|comment| format!("{indent}{comment}")));
        changed = true;
    } else {
        out.extend(
            text.lines[now.start..first]
                .iter()
                .map(|line| line.to_string()),
        );
    }
    match (trailing(source.lines[from]), trailing(line)) {
        (Some(comment), None) => {
            if text.text.contains(&comment) {
                return None;
            }
            out.push(format!("{} {comment}", line.trim_end()));
            changed = true;
        }
        _ => out.push(line.to_string()),
    }
    let start = text.starts[now.start];
    let end = text.starts[first] + TextSize::of(line);
    changed.then(|| (TextRange::new(start, end.max(start)), out.join("\n")))
}

/// `work` run while a batch reads each of `texts` (with the spans of its
/// pointers) once.
fn in_batches<T>(
    host: &dyn ConfigLanguage,
    texts: &[(&str, Vec<String>)],
    work: &dyn Fn() -> T,
) -> T {
    match texts.split_first() {
        Some(((text, pointers), rest)) => {
            paths::with_spans(host, text, pointers, || in_batches(host, rest, work))
        }
        None => work(),
    }
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
    told: SaveTold<'_>,
) -> String {
    note_many(Work::SaveItemJudged, moved.len());
    // Each source text split in lines once.
    let mut sources: Vec<Lines<'_>> = Vec::new();
    for item in moved {
        if !sources
            .iter()
            .any(|source| std::ptr::eq(source.text, item.source.text))
        {
            sources.push(Lines::of(item.source.text));
        }
    }
    let write = |text: &Lines<'_>, item: &Moved<'_>| -> Option<(TextRange, String)> {
        // Not an item whose keys the edit put in another order: its
        // source's text would put them back (the read back compares
        // values, not their order).
        let (now, was) = (
            before.pointer(&item.to)?,
            item.source.value.pointer(&item.from)?,
        );
        if !codec::same_order(now, was) {
            return None;
        }
        let source = sources
            .iter()
            .find(|source| std::ptr::eq(source.text, item.source.text))?;
        write(host, text, source, item)
    };
    let inside = |inner: &str, outer: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    // No form changed the save did not change already.
    let changed = host.changed_form(previous, &saved, told);
    let reads = |text: &str| {
        codec::read(host, text, shape).ok().as_ref() == Some(before) && {
            let now = host.changed_form(previous, text, told);
            now.is_none() || now == changed
        }
    };
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
    // The text written and every source text (the host's last, an
    // earlier one items were pasted back from) each read once.
    let batched = |text: &str, work: &dyn Fn() -> Vec<Option<(TextRange, String)>>| {
        let mut texts: Vec<(&str, Vec<String>)> = vec![(text, spans_of(text, false))];
        for source in &sources {
            texts.push((source.text, spans_of(source.text, true)));
        }
        in_batches(host, &texts, work)
    };
    // The comments of the items not written back whole (their values
    // changed since), as their sources write them, where the text then
    // reads the same.
    // Their writes in `text`, inside a batch of its spans.
    let comment_edits = |text: &str| -> Vec<Option<(TextRange, String)>> {
        let lines = Lines::of(text);
        moved
            .iter()
            .map(|item| {
                let source = sources
                    .iter()
                    .find(|source| std::ptr::eq(source.text, item.source.text))?;
                comments(host, &lines, source, item)
            })
            .collect()
    };
    let with_edits = |text: String, edits: Vec<Option<(TextRange, String)>>| -> String {
        let mut edits: Vec<(TextRange, String)> = edits.into_iter().flatten().collect();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start()));
        edits.dedup_by(|left, right| left.0 == right.0);
        let apart = edits
            .windows(2)
            .all(|pair| pair[1].0.end() <= pair[0].0.start());
        if edits.is_empty() || !apart {
            return text;
        }
        match all_made(&text, &edits) {
            Some(all) if reads(&all) => all,
            _ => text,
        }
    };
    let with_comments = |text: String| -> String {
        let edits = batched(&text, &|| comment_edits(&text));
        with_edits(text, edits)
    };
    // The writes of `saved`, and its comments' for a save left as it is,
    // from one read of its spans.
    let saved_comments = std::cell::RefCell::new(Vec::new());
    let all_writes = batched(&saved, &|| {
        let lines = Lines::of(&saved);
        saved_comments.replace(comment_edits(&saved));
        moved.iter().map(|item| write(&lines, item)).collect()
    });
    // The outermost items with a write, all at once (`moved` is in
    // pointer order: an item before the ones inside it). An item inside
    // one with none (a new map the links name, written anew) is written
    // on its own.
    let mut outer: Vec<&str> = Vec::new();
    let mut writes: Vec<(TextRange, String)> = Vec::new();
    for (item, write) in moved.iter().zip(&all_writes) {
        if let Some(write) = write
            && !outer.iter().any(|outer| inside(&item.to, outer))
        {
            outer.push(&item.to);
            writes.push(write.clone());
        }
    }
    writes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start()));
    let apart = writes
        .windows(2)
        .all(|pair| pair[1].0.end() <= pair[0].0.start());
    if apart
        && !writes.is_empty()
        && let Some(all) = all_made(&saved, &writes)
        && reads(&all)
    {
        return with_comments(all);
    }
    // Else one at a time, an outer item first: an item inside one
    // written is written with it.
    let differing: Vec<&Moved<'_>> = moved
        .iter()
        .zip(&all_writes)
        .filter(|(item, write)| {
            write.is_some() || before.pointer(&item.to) != item.source.value.pointer(&item.from)
        })
        .map(|(item, _)| item)
        .collect();
    if differing.len() > MOST_TRIED {
        return with_edits(saved, saved_comments.take());
    }
    let mut text = saved;
    let mut rewritten = false;
    let mut written: Vec<&str> = Vec::new();
    let mut tried: Vec<&Moved<'_>> = differing;
    tried.reverse();
    while let Some(item) = tried.pop() {
        if written.iter().any(|outer| inside(&item.to, outer)) {
            continue;
        }
        let whole = write(&Lines::of(&text), item)
            .and_then(|(range, replaced)| {
                let mut next = text.clone();
                slice::replace(&mut next, range, &replaced)?;
                Some(next)
            })
            .filter(|next| reads(next));
        if let Some(next) = whole {
            text = next;
            rewritten = true;
            written.push(&item.to);
            continue;
        }
        // An item the edit changed inside (a key added to it, an item
        // removed): its source's text with that change made there, as a
        // save of its source would make it, so the rest of it keeps its
        // forms; then the items inside it the edit moved, each from its
        // own source.
        if let Some(next) = changed_inside(host, &text, item, before, shape)
            && reads(&next)
        {
            text = next;
            rewritten = true;
            let inner: Vec<&Moved<'_>> = moved
                .iter()
                .filter(|inner| {
                    inside(&inner.to, &item.to)
                        && !tried.iter().any(|next| std::ptr::eq(*next, *inner))
                })
                .collect();
            tried.extend(inner.into_iter().rev());
        }
    }
    if rewritten {
        with_comments(text)
    } else {
        with_edits(text, saved_comments.take())
    }
}

/// Whether `patched` (its source `text` patched so the item at `from`
/// holds `now`, where it held `held`) writes each scalar of the item
/// that holds what it held as `text` does, and there is one: the patch
/// made only the change, to an item still holding some of what it held.
fn forms_kept(
    host: &dyn ConfigLanguage,
    (text, patched): (&str, &str),
    from: &str,
    (held, now): (&Value, &Value),
) -> bool {
    fn leaves(held: &Value, now: &Value, pointer: &mut String, out: &mut Vec<String>) {
        let length = pointer.len();
        match (held, now) {
            (Value::Object(held), Value::Object(now)) => {
                for (key, child) in held {
                    if let Some(other) = now.get(key) {
                        pointer.push('/');
                        pointer.push_str(&pointer_key(key));
                        leaves(child, other, pointer, out);
                        pointer.truncate(length);
                    }
                }
            }
            (Value::Array(held), Value::Array(now)) => {
                for (index, (child, other)) in held.iter().zip(now).enumerate() {
                    pointer.push('/');
                    pointer.push_str(&index.to_string());
                    leaves(child, other, pointer, out);
                    pointer.truncate(length);
                }
            }
            (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {}
            _ if held == now => out.push(pointer.clone()),
            _ => {}
        }
    }
    let mut pointers = Vec::new();
    leaves(held, now, &mut from.to_string(), &mut pointers);
    // An item holding none of what it held is no change made inside it.
    if pointers.is_empty() {
        return false;
    }
    let paths: Vec<FieldPath> = pointers.iter().map(FieldPath::from).collect();
    let written = |source: &str| -> Vec<Option<String>> {
        host.value_spans(source, &paths)
            .into_iter()
            .zip(&pointers)
            .map(|(span, pointer)| {
                let span = span.or_else(|| codec::inline_location(host, source, pointer, false))?;
                let range = written_text_range(host, source, span)?;
                slice::slice(source, range).map(|written| written.trim().to_string())
            })
            .collect()
    };
    written(text) == written(patched)
}

/// Whether the keys `now` shares with `held` come in `held`'s order, all
/// the way down (a key added or removed changes no order).
fn kept_order(now: &Value, held: &Value) -> bool {
    match (now, held) {
        (Value::Object(now), Value::Object(held)) => {
            now.keys()
                .filter(|key| held.contains_key(*key))
                .eq(held.keys().filter(|key| now.contains_key(*key)))
                && now
                    .iter()
                    .all(|(key, value)| held.get(key).is_none_or(|other| kept_order(value, other)))
        }
        (Value::Array(now), Value::Array(held)) => now
            .iter()
            .zip(held)
            .all(|(now, held)| kept_order(now, held)),
        _ => true,
    }
}

/// `text` with the item `item` written as its source text writes it,
/// patched there to hold what `before` holds at its place; `None` when
/// it holds what it held, or no patch makes its source hold it.
fn changed_inside(
    host: &dyn ConfigLanguage,
    text: &str,
    item: &Moved<'_>,
    before: &Value,
    shape: Option<&Value>,
) -> Option<String> {
    let now = before.pointer(&item.to)?;
    let was = item.source.value;
    let held = was.pointer(&item.from)?;
    // A map still a map, a list still a list: its source's text is the
    // start of its own.
    let kind = |value: &Value| (value.is_object(), value.is_array());
    if held == now || kind(held) != kind(now) || !kept_order(now, held) {
        return None;
    }
    let mut changed = was.clone();
    *changed.pointer_mut(&item.from)? = now.clone();
    let patched = super::super::patched(host, item.source.text, was, &changed, shape);
    let patched = patched?;
    if !forms_kept(host, (item.source.text, &patched), &item.from, (held, now)) {
        return None;
    }
    let source = Lines::of(&patched);
    let (range, replaced) = write(host, &Lines::of(text), &source, item)?;
    let mut next = text.to_string();
    slice::replace(&mut next, range, &replaced)?;
    Some(next)
}

/// `text` with `edits` (sorted from the last, apart) made; `None` when
/// one is no cut of it.
fn all_made(text: &str, edits: &[(TextRange, String)]) -> Option<String> {
    let mut all = text.to_string();
    for (range, written) in edits {
        slice::replace(&mut all, *range, written)?;
    }
    Some(all)
}
