//! A moved or copied item written back whole as its source wrote it: an
//! item on lines of its own as those lines (the comment lines right above
//! it, its key's or item's line, the lines its value runs on: its
//! comments and every form inside it included), indented where it now is,
//! the separator after it its place's; an item sharing its line from its
//! key (or, for a list item, its value) to its value's end. A save that
//! wrote the item anew (its block, table or element changed) keeps it as
//! written, where that text reads back the same there; else its scalars
//! are written back one by one (see `kept`).

use super::super::super::codec::{self, byte_range, written_range};
use super::super::super::language::ConfigLanguage;
use super::super::super::paths;
use super::{Moved, line_indent, reindented};
use serde_json::Value;
use std::ops::Range;

/// A text split in lines once, for every item placed in it.
struct Lines<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
    /// Where each line starts.
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn of(text: &'a str) -> Self {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for line in &lines {
            starts.push(at);
            at += line.len() + 1;
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
    Lines(Range<usize>),
    /// Sharing its line (in a container written on one line): from its
    /// key's start (a list item's: its value's) to its value's end.
    Inline(Range<usize>),
}

/// Where the item at `pointer` is written in `text`.
fn place(host: &dyn ConfigLanguage, text: &Lines<'_>, pointer: &str) -> Option<Place> {
    if let Some(lines) = codec::own_lines(host, text.text, &text.lines, pointer) {
        let first = text.lines[lines.start];
        let start = text.starts[lines.start] + first.len() - first.trim_start().len();
        let last = lines.end - 1;
        return Some(Place::Lines(
            start..text.starts[last] + text.lines[last].len(),
        ));
    }
    inline(host, text, pointer).map(Place::Inline)
}

/// The bytes of the item at `pointer` in `text` from its key's start (a
/// list item's: its value's) to its value's end.
fn inline(host: &dyn ConfigLanguage, text: &Lines<'_>, pointer: &str) -> Option<Range<usize>> {
    let value = paths::memo_value_span(host, text.text, pointer)?;
    let value = written_range(host, text.text, value)?;
    let start = paths::memo_key_span(host, text.text, pointer)
        .and_then(|key| byte_range(text.text, key))
        .map_or(value.start, |key| key.start.min(value.start));
    Some(start..value.end)
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
) -> Option<(Range<usize>, String)> {
    // The source's text from `was`, its key written as the place's (a
    // key renamed in the view keeps its new name).
    let renamed = |was: &Range<usize>, now: &Range<usize>| -> Option<String> {
        let key = |text: &str, pointer: &str| {
            paths::memo_key_span(host, text, pointer).and_then(|key| byte_range(text, key))
        };
        let mut written = source.text[was.clone()].to_string();
        if let (Some(old), Some(new)) = (key(source.text, &item.from), key(text.text, &item.to)) {
            if old.start < was.start
                || old.end > was.end
                || new.start < now.start
                || new.end > now.end
            {
                return None;
            }
            written.replace_range(old.start - was.start..old.end - was.start, &text.text[new]);
        }
        Some(written)
    };
    let (now, written) = match (
        place(host, text, &item.to)?,
        place(host, source, &item.from)?,
    ) {
        (Place::Lines(now), Place::Lines(was)) => {
            let written = renamed(&was, &now)?;
            let written = written.as_str();
            let place_last = &text.text[now.clone()]
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
            let indent = line_indent(text.text, now.start);
            let from = line_indent(source.text, was.start);
            let written = reindented(&written, from, indent);
            // The comment lines the place has above the item that its
            // source has not (another item's, which the save put there)
            // stay.
            let above = |lines: &Lines<'_>, start: usize, pointer: &str| -> Vec<String> {
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
            let theirs = above(source, was.start, &item.from);
            let others: Vec<String> = above(text, now.start, &item.to)
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
            let written = renamed(&was, &now)?;
            (now, written)
        }
        // An item its source writes on a line of its own, now sharing
        // its line (the save wrote it after a list item's marker): from
        // its key to its value's end, a value on one line only.
        (Place::Inline(now), Place::Lines(_)) => {
            let was = inline(host, source, &item.from)?;
            let written = renamed(&was, &now)?;
            if written.contains('\n') {
                return None;
            }
            (now, written)
        }
        _ => return None,
    };
    (text.text[now.clone()] != written).then_some((now, written))
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
) -> Option<(Range<usize>, String)> {
    let now = codec::own_lines(host, text.text, &text.lines, &item.to)?;
    let was = codec::own_lines(host, source.text, &source.lines, &item.from)?;
    let (first, from) = (
        first_line(host, text.text, &item.to)?,
        first_line(host, source.text, &item.from)?,
    );
    if !(now.contains(&first) && was.contains(&from)) {
        return None;
    }
    let trailing = |line: &'_ str| -> Option<String> {
        host.comment_start(line)
            .map(|at| line[at..].trim().to_string())
            .filter(|comment| !comment.is_empty())
    };
    let above: Vec<&str> = source.lines[was.start..from]
        .iter()
        .map(|line| line.trim())
        .collect();
    let line = text.lines[first];
    let indent = &line[..line.len() - line.trim_start().len()];
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
    changed.then(|| (start..text.starts[first] + line.len(), out.join("\n")))
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
) -> String {
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
    let write = |text: &Lines<'_>, item: &Moved<'_>| -> Option<(Range<usize>, String)> {
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
    // The text written and every source text (the host's last, an
    // earlier one items were pasted back from) each read once.
    let batched = |text: &str, work: &dyn Fn() -> Vec<Option<(Range<usize>, String)>>| {
        let mut texts: Vec<(&str, Vec<String>)> = vec![(text, spans_of(text, false))];
        for source in &sources {
            texts.push((source.text, spans_of(source.text, true)));
        }
        in_batches(host, &texts, work)
    };
    // The comments of the items not written back whole (their values
    // changed since), as their sources write them, where the text then
    // reads the same.
    let with_comments = |text: String| -> String {
        let lines = Lines::of(&text);
        let mut edits: Vec<(Range<usize>, String)> = batched(&text, &|| {
            moved
                .iter()
                .map(|item| {
                    let source = sources
                        .iter()
                        .find(|source| std::ptr::eq(source.text, item.source.text))?;
                    comments(host, &lines, source, item)
                })
                .collect()
        })
        .into_iter()
        .flatten()
        .collect();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        edits.dedup_by(|left, right| left.0 == right.0);
        let apart = edits
            .windows(2)
            .all(|pair| pair[1].0.end <= pair[0].0.start);
        if edits.is_empty() || !apart {
            return text;
        }
        let mut all = text.clone();
        for (range, written) in &edits {
            all.replace_range(range.clone(), written);
        }
        if reads(&all) { all } else { text }
    };
    let all_writes = batched(&saved, &|| {
        let lines = Lines::of(&saved);
        moved.iter().map(|item| write(&lines, item)).collect()
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
            return with_comments(all);
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
        return with_comments(saved);
    }
    let mut text = saved;
    let mut written: Vec<&str> = Vec::new();
    for item in differing {
        if written.iter().any(|outer| inside(&item.to, outer)) {
            continue;
        }
        let Some((range, replaced)) = write(&Lines::of(&text), item) else {
            continue;
        };
        let mut next = text.clone();
        next.replace_range(range, &replaced);
        if reads(&next) {
            text = next;
            written.push(&item.to);
        }
    }
    with_comments(text)
}
