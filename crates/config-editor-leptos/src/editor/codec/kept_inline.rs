//! One-line containers kept as the host wrote them through a structural save.
//!
//! The host serializer writes `allow: [a, b]` or `heartbeat = { … }` over
//! several lines, which match no host line, so a line patch next to one
//! failed and the whole host was rewritten. Each one-line container is
//! hidden behind a marker scalar for the line patch, then written back as it
//! was; one the edit changed is written out by the serializer instead.

use super::super::flow_syntax::Flow;
use super::super::language::{ConfigLanguage, OneLine, Span};
use super::escape;
use serde_json::Value;
use std::ops::Range;

/// What the line patch sees: the host and both values with markers in place
/// of the kept containers, and each marker's original text.
pub(super) struct Kept {
    pub host: String,
    pub old: Value,
    pub new: Value,
    /// Each marker with what the host wrote before it, the original text,
    /// and the text that stands for it as a plain value.
    texts: Vec<(String, String, String, String)>,
}

impl Kept {
    /// `patched` with each marker replaced by the text it stands for: the
    /// original where the host's own form (`tcp = <marker>`) is still there,
    /// else the value's text.
    pub fn restore(&self, patched: &str) -> String {
        let mut restored = patched.to_string();
        for (marker, written, text, value) in &self.texts {
            if !written.is_empty() {
                restored = replace_marker(&restored, written, marker, text);
            }
            restored = replace_marker(&restored, "", marker, value);
        }
        restored
    }
}

/// `text` with each `marker` written after `before` replaced by `with`,
/// together with the quotes the serializer may have put around it (the
/// same character on both sides, neither a word's nor a space).
fn replace_marker(text: &str, before: &str, marker: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let (head, tail) = (&rest[..at], &rest[at + marker.len()..]);
        let quote = head
            .chars()
            .next_back()
            .filter(|quote| {
                !quote.is_alphanumeric() && !quote.is_whitespace() && tail.starts_with(*quote)
            })
            .map_or(0, char::len_utf8);
        let head_end = head.len() - quote;
        if head[..head_end].ends_with(before) {
            out.push_str(&head[..head_end - before.len()]);
            out.push_str(with);
            rest = &tail[quote..];
        } else {
            out.push_str(&rest[..at + marker.len()]);
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// Text no host contains, ending so one marker is never a prefix of another.
fn marker(index: usize) -> String {
    format!("config-editor-kept-{index}-end")
}

/// The kept containers of `previous`, or `None` when it has none.
pub(super) fn hide(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
) -> Option<Kept> {
    if previous.contains("config-editor-kept-") {
        return None;
    }
    // Lines holding a whole bracketed or tagged container: only a container
    // whose values all sit on one of them is located (a span lookup parses
    // the host, too slow to do for every container of a large one).
    let flow = Flow::of(language);
    let pairs: Vec<(char, char)> = flow.map().into_iter().chain(flow.list()).collect();
    let lines: Vec<&str> = previous
        .lines()
        .filter(|line| {
            pairs
                .iter()
                .any(|(open, close)| line.contains(*open) && line.contains(*close))
                || language.writes_container_on_line(line)
        })
        .collect();
    // A list opened at a line's end (`allow = [`) spans lines; the
    // serializer writes it on one.
    let multi_line_lists = flow.list().is_some_and(|(open, _)| {
        previous.lines().any(|line| {
            line[..super::comment_start(language, line)]
                .trim_end()
                .ends_with(open)
        })
    });
    // A map written inline over lines (YAML's `tcp: {`), as the language
    // tells: the serializer writes it another way.
    let multi_line_maps = previous
        .lines()
        .any(|line| language.flow_map_opener(line[..super::comment_start(language, line)].trim()));
    // Strings written over lines (a YAML block, a TOML `"""`, a heredoc):
    // the serializer writes them on one.
    let host_lines: Vec<&str> = previous.split('\n').collect();
    let bodies: Vec<String> = language
        .saved_string_lines(previous)
        .into_iter()
        // With the line that opens it, which may hold its first words (`m: "a`).
        .filter_map(|range| {
            host_lines
                .get(range.start.saturating_sub(1)..range.end)
                .map(|lines| lines.join("\n"))
        })
        .collect();
    let string_bodies = !bodies.is_empty();
    // Only where a reference stands as a value (`key: *p`, `- *p`).
    let references = previous
        .lines()
        .any(|line| super::reference_on(language, &flow, line));
    if lines.is_empty() && !multi_line_lists && !multi_line_maps && !string_bodies && !references {
        return None;
    }
    let mut candidates = Vec::new();
    collect(
        old,
        &lines,
        (multi_line_lists, multi_line_maps),
        String::new(),
        &mut candidates,
    );
    // Containers written by reference (YAML's `- *p`): kept as written.
    if references {
        collect_containers(old, String::new(), &mut candidates);
        candidates.sort();
        candidates.dedup();
        // Parents before children, as the outermost-first pass expects.
        candidates.sort_by_key(|pointer| pointer.matches('/').count());
    }
    let spans = language.value_spans(previous, &candidates);
    let mut found: Vec<(String, OneLine)> = Vec::new();
    // Outermost first (`collect` lists parents before their children).
    for (pointer, span) in candidates.into_iter().zip(spans) {
        let inside_found = found.iter().any(|(outer, _)| {
            pointer.starts_with(outer.as_str()) && pointer[outer.len()..].starts_with('/')
        });
        if inside_found {
            continue;
        }
        let one_line = span
            .and_then(|span| {
                braced_one_line(language, previous, span).or_else(|| {
                    // A list over lines that stays a list but changes is
                    // edited in place (see `lists`); kept, deleted or
                    // replaced by a value, it is hidden like the others.
                    let now = new.pointer(&pointer);
                    let was = old.pointer(&pointer);
                    let same_kind = now.is_some_and(|now| {
                        now.is_array() && was.is_some_and(Value::is_array)
                            || now.is_object() && was.is_some_and(Value::is_object)
                    });
                    (now == was || !same_kind)
                        .then(|| multi_line_list(language, previous, span))
                        .flatten()
                })
            })
            .or_else(|| {
                // Written by reference, and the same in the new value.
                let range = super::byte_range(previous, span?)?;
                let length = language.reference_len(&previous[range.start..])?;
                (new.pointer(&pointer) == old.pointer(&pointer))
                    .then_some(range.start..range.start + length)
            })
            .map(|range| OneLine {
                range: range.clone(),
                written: String::new(),
                value: range,
            })
            .or_else(|| language.one_line_container(previous, &pointer));
        if let Some(one_line) = one_line {
            found.push((pointer, one_line));
        }
    }
    if string_bodies {
        let mut strings = Vec::new();
        collect_strings(old, String::new(), &mut strings);
        // Only strings whose text is in one of the bodies: a span lookup per
        // string of a large host is too slow.
        strings.retain(|pointer| {
            old.pointer(pointer)
                .and_then(Value::as_str)
                .is_some_and(|text| {
                    text.contains('\n')
                        && text
                            .lines()
                            .map(str::trim)
                            .filter(|line| !line.is_empty())
                            .any(|line| bodies.iter().any(|body| body.contains(line)))
                })
                || old
                    .pointer(pointer)
                    .and_then(Value::as_str)
                    .is_some_and(|text| {
                        // A folded block (`>-`) holds one line of text over lines.
                        let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        !words.is_empty()
                            && bodies.iter().any(|body| {
                                body.split_whitespace()
                                    .collect::<Vec<_>>()
                                    .join(" ")
                                    .contains(&words)
                            })
                    })
        });
        let spans = language.value_spans(previous, &strings);
        for (pointer, span) in strings.into_iter().zip(spans) {
            let inside_found = found.iter().any(|(outer, _)| {
                pointer.starts_with(outer.as_str()) && pointer[outer.len()..].starts_with('/')
            });
            let Some(mut range) = span.and_then(|span| super::byte_range(previous, span)) else {
                continue;
            };
            range.end = super::through_long_quote(language, previous, range.clone());
            while range.end > range.start && previous[..range.end].ends_with('\n') {
                range.end -= 1;
            }
            // Where the language's string really ends (XML text: through
            // the line break before its end tag).
            range.end = language.kept_value_end(previous, range.end);
            if !inside_found && previous[range.clone()].contains('\n') {
                found.push((
                    pointer,
                    OneLine {
                        range: range.clone(),
                        written: String::new(),
                        value: range,
                    },
                ));
            }
        }
    }
    if found.is_empty() {
        return None;
    }
    found.sort_by_key(|(_, one_line)| one_line.range.start);
    let mut kept = Kept {
        host: previous.to_string(),
        old: old.clone(),
        new: new.clone(),
        texts: Vec::new(),
    };
    let mut values = Vec::new();
    // Replaced from the end, so earlier ranges keep their offsets.
    for (index, (pointer, one_line)) in found.into_iter().enumerate().rev() {
        let name = marker(index);
        let marker = Value::String(name.clone());
        let text = previous[one_line.range.clone()].to_string();
        let value = previous[one_line.value.clone()].to_string();
        let written = format!("{}{}", one_line.written, language.format_value(&marker));
        kept.host.replace_range(one_line.range, &written);
        let slot = kept.old.pointer_mut(&pointer)?;
        values.push((
            pointer.clone(),
            std::mem::replace(slot, marker.clone()),
            marker,
        ));
        kept.texts.push((name, one_line.written, text, value));
    }
    // The new value holds a kept container where it was, or where list
    // items moved it (the first of a list deleted); an equal container under
    // another key is its own and stays.
    values.reverse();
    let mut used = Vec::new();
    for (pointer, container, marker) in values {
        let mut places = Vec::new();
        same_place(
            &kept.new,
            &pointer_segments_of(&pointer),
            String::new(),
            &mut places,
        );
        // Its own pointer first, then shifted items in order.
        places.sort_by_key(|place| *place != pointer);
        if let Some(place) = places
            .into_iter()
            .find(|place| !used.contains(place) && kept.new.pointer(place) == Some(&container))
        {
            if let Some(slot) = kept.new.pointer_mut(&place) {
                *slot = marker;
            }
            used.push(place);
        }
    }
    Some(kept)
}

/// The segments of a JSON pointer, still escaped.
fn pointer_segments_of(pointer: &str) -> Vec<String> {
    pointer.split('/').skip(1).map(str::to_string).collect()
}

/// Pointers in `value` along `segments`, where a list index may be any index
/// of that list.
fn same_place(value: &Value, segments: &[String], pointer: String, out: &mut Vec<String>) {
    let Some((first, rest)) = segments.split_first() else {
        out.push(pointer);
        return;
    };
    match value {
        Value::Array(items) if crate::editor::language::pointer_index(first).is_some() => {
            for (index, item) in items.iter().enumerate() {
                same_place(item, rest, format!("{pointer}/{index}"), out);
            }
        }
        Value::Object(map) => {
            let key = first.replace("~1", "/").replace("~0", "~");
            if let Some(child) = map.get(&key) {
                same_place(child, rest, format!("{pointer}/{first}"), out);
            }
        }
        _ => {}
    }
}

/// Pointers of the containers in `old` that may be written on one of
/// `lines`, parents before children.
fn collect(
    old: &Value,
    lines: &[&str],
    (multi_line_lists, multi_line_maps): (bool, bool),
    pointer: String,
    out: &mut Vec<String>,
) {
    if !pointer.is_empty()
        && (old.is_object() || old.is_array())
        && ((multi_line_lists && old.is_array())
            || (multi_line_maps && old.is_object())
            || on_one_line(&pointer, old, lines))
    {
        out.push(pointer.clone());
    }
    match old {
        Value::Object(map) => {
            for (key, value) in map {
                collect(
                    value,
                    lines,
                    (multi_line_lists, multi_line_maps),
                    format!("{pointer}/{}", escape(key)),
                    out,
                );
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                collect(
                    value,
                    lines,
                    (multi_line_lists, multi_line_maps),
                    format!("{pointer}/{index}"),
                    out,
                );
            }
        }
        _ => {}
    }
}

/// Pointers of the containers in `value`.
fn collect_containers(value: &Value, pointer: String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if !pointer.is_empty() {
                out.push(pointer.clone());
            }
            for (key, child) in map {
                collect_containers(child, format!("{pointer}/{}", escape(key)), out);
            }
        }
        Value::Array(items) => {
            if !pointer.is_empty() {
                out.push(pointer.clone());
            }
            for (index, child) in items.iter().enumerate() {
                collect_containers(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// Pointers of the strings in `value`.
fn collect_strings(value: &Value, pointer: String, out: &mut Vec<String>) {
    match value {
        Value::String(_) if !pointer.is_empty() => out.push(pointer),
        Value::Object(map) => {
            for (key, child) in map {
                collect_strings(child, format!("{pointer}/{}", escape(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_strings(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// Leaves of a container looked for on one line; a larger one is not
/// written on one.
const MAX_ONE_LINE_LEAVES: usize = 32;

/// Whether one of `lines` holds every key and scalar of `container` as text,
/// and its own key unless it is an array item.
fn on_one_line(pointer: &str, container: &Value, lines: &[&str]) -> bool {
    fn leaves(value: &Value, out: &mut Vec<String>) -> bool {
        match value {
            Value::Object(map) => map.iter().all(|(key, child)| {
                out.push(key.clone());
                leaves(child, out)
            }),
            Value::Array(items) => items.iter().all(|child| leaves(child, out)),
            Value::String(text) => {
                out.push(text.clone());
                out.len() <= MAX_ONE_LINE_LEAVES
            }
            Value::Null => true,
            other => {
                out.push(other.to_string());
                out.len() <= MAX_ONE_LINE_LEAVES
            }
        }
    }
    let own = pointer.rsplit('/').next().unwrap_or_default();
    let mut texts = Vec::new();
    if crate::editor::language::pointer_index(own).is_none() {
        texts.push(own.replace("~1", "/").replace("~0", "~"));
    }
    leaves(container, &mut texts)
        && texts.len() <= MAX_ONE_LINE_LEAVES
        && lines
            .iter()
            .any(|line| texts.iter().all(|text| line.contains(text.as_str())))
}

/// The byte range of the value marked by `span` when the host wrote it as
/// a `{…}` or `[…]` on one line.
fn braced_one_line(
    language: &dyn ConfigLanguage,
    previous: &str,
    span: Span,
) -> Option<Range<usize>> {
    let line_start: usize = previous
        .split_inclusive('\n')
        .take(span.line.checked_sub(1)?)
        .map(str::len)
        .sum();
    let line = previous[line_start..].split('\n').next()?;
    let start = line
        .char_indices()
        .nth(span.start_column.saturating_sub(1))
        .map_or(line.len(), |(index, _)| index);
    let start = start + language.value_properties_len(&line[start..]);
    let text = &line[start..];
    let flow = Flow::of(language);
    let close = flow.closer_of(text.chars().next()?)?;
    let end = start + flow.matching_close(text)? + close.len_utf8();
    Some(line_start + start..line_start + end)
}

/// The byte range of the list marked by `span` when it opens at its line's
/// end and closes on a later line (`allow = [` … `]`), comments skipped.
pub(super) fn multi_line_list(
    language: &dyn ConfigLanguage,
    previous: &str,
    span: Span,
) -> Option<Range<usize>> {
    let line_start: usize = previous
        .split_inclusive('\n')
        .take(span.line.checked_sub(1)?)
        .map(str::len)
        .sum();
    let line = previous[line_start..].split('\n').next()?;
    let start = line_start
        + line
            .char_indices()
            .nth(span.start_column.saturating_sub(1))
            .map_or(line.len(), |(index, _)| index);
    let flow = Flow::of(language);
    if !previous[start..]
        .chars()
        .next()
        .is_some_and(|open| flow.closer_of(open).is_some())
    {
        return None;
    }
    let mut depth = 0usize;
    let mut offset = line_start;
    for line in previous[line_start..].split_inclusive('\n') {
        let code_end = offset + super::comment_start(language, line.trim_end_matches('\n'));
        let from = start.max(offset);
        let code = &previous[from..code_end.max(from)];
        let mut at = 0;
        while let Some(character) = code[at..].chars().next() {
            if flow.quote(character).is_some() {
                at = flow.quote_end(code, at);
                continue;
            }
            if flow.closer_of(character).is_some() {
                depth += 1;
            } else if flow.opener_of(character).is_some() {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end = from + at + character.len_utf8();
                    return previous[start..end].contains('\n').then_some(start..end);
                }
            }
            at += character.len_utf8();
        }
        offset += line.len();
    }
    None
}
