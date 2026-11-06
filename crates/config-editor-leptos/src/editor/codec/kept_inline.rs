//! One-line containers kept as the host wrote them through a structural save.
//!
//! The host serializer writes `allow: [a, b]` or `heartbeat = { … }` over
//! several lines, which match no host line, so a line patch next to one
//! failed and the whole host was rewritten. Each one-line container is
//! hidden behind a marker scalar for the line patch, then written back as it
//! was; one the edit changed is written out by the serializer instead.

use super::super::inline::matching_close;
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
            for quote in ["\"", "'", ""] {
                if !written.is_empty() {
                    restored = restored.replace(&format!("{written}{quote}{marker}{quote}"), text);
                }
                restored = restored.replace(&format!("{quote}{marker}{quote}"), value);
            }
        }
        restored
    }
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
    let lines: Vec<&str> = previous
        .lines()
        .filter(|line| {
            (line.contains('{') && line.contains('}'))
                || (line.contains('[') && line.contains(']'))
                || line.matches('<').count() >= 4
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut candidates = Vec::new();
    collect(old, &lines, String::new(), &mut candidates);
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
            .and_then(|span| braced_one_line(previous, span))
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
        values.push((std::mem::replace(slot, marker.clone()), marker));
        kept.texts.push((name, one_line.written, text, value));
    }
    // The new value holds a kept container wherever it holds an equal one:
    // items may have moved (the first of a list deleted).
    values.reverse();
    hide_equal(&mut kept.new, &values);
    Some(kept)
}

/// Replaces each container in `value` equal to a kept one with its marker.
fn hide_equal(value: &mut Value, kept: &[(Value, Value)]) {
    if let Some((_, marker)) = kept.iter().find(|(container, _)| container == value) {
        *value = marker.clone();
        return;
    }
    match value {
        Value::Object(map) => map.values_mut().for_each(|child| hide_equal(child, kept)),
        Value::Array(items) => items.iter_mut().for_each(|child| hide_equal(child, kept)),
        _ => {}
    }
}

/// Pointers of the containers in `old` that may be written on one of
/// `lines`, parents before children.
fn collect(old: &Value, lines: &[&str], pointer: String, out: &mut Vec<String>) {
    if !pointer.is_empty()
        && (old.is_object() || old.is_array())
        && on_one_line(&pointer, old, lines)
    {
        out.push(pointer.clone());
    }
    match old {
        Value::Object(map) => {
            for (key, value) in map {
                collect(value, lines, format!("{pointer}/{}", escape(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                collect(value, lines, format!("{pointer}/{index}"), out);
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
    if own.parse::<usize>().is_err() {
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
fn braced_one_line(previous: &str, span: Span) -> Option<Range<usize>> {
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
    let text = &line[start..];
    let close = match text.chars().next()? {
        '{' => '}',
        '[' => ']',
        _ => return None,
    };
    let end = start + matching_close(text, close)? + 1;
    Some(line_start + start..line_start + end)
}
