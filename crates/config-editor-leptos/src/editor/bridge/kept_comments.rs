//! Which item each comment of the host belongs to, and the check that a
//! save keeps the comments of the items it leaves alone (P2: comments
//! outside edited lines survive, or the save waits with a reason).
//!
//! A comment belongs to a line of the host's text, and through it to the
//! items written on that line (each item whose key or value starts there):
//! - a comment after code on its line (a trailing comment) to that line;
//! - a comment before code on its line (`<!-- c --> <a>1</a>`) to that
//!   line;
//! - a comment on lines of its own (a leading comment) to the next line
//!   that writes an item, unless it is indented deeper than that line
//!   (it closes the block above it) or no line below writes one: then to
//!   the last line above that writes one.
//!
//! The line is left alone when each of its items is still in the save:
//! where it was, or where the links say the edit moved it (or a map or
//! list holding it), a scalar or list item holding what it held, a map or
//! list still one. A comment of a line left alone must still be in the
//! saved text.

use super::super::codec;
use super::super::item_sources::ItemSource;
use super::super::language::{ConfigLanguage, Span};
use super::super::paths;
use super::ItemMap;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

/// A comment's words: its text without the language's markers, each run
/// of blanks as one space.
fn words(language: &dyn ConfigLanguage, comment: &str) -> String {
    let config = language.config();
    let mut text = comment.trim();
    if let Some((open, close)) = config.block_comment
        && let Some(inner) = text.strip_prefix(open)
    {
        text = inner.strip_suffix(close).unwrap_or(inner);
    }
    if let Some(inner) = config
        .line_comments
        .iter()
        .find_map(|marker| text.strip_prefix(marker))
    {
        text = inner;
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The words of each comment `text` writes, with how many write them.
fn counted(language: &dyn ConfigLanguage, text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for range in language.comments(text) {
        if let Some(comment) = text.get(range) {
            *counts.entry(words(language, comment)).or_insert(0) += 1;
        }
    }
    counts
}

/// The comments `text` writes, by their words (see [`words`]).
pub(super) fn comment_words(language: &dyn ConfigLanguage, text: &str) -> Vec<String> {
    language
        .comments(text)
        .into_iter()
        .filter_map(|range| text.get(range).map(|comment| words(language, comment)))
        .collect()
}

/// Every pointer of `value` (the root left out), parents first.
fn pointers(value: &Value, pointer: &mut String, out: &mut Vec<String>) {
    let length = pointer.len();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&codec::escape(key));
                out.push(pointer.clone());
                pointers(child, pointer, out);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                pointer.push('/');
                pointer.push_str(&index.to_string());
                out.push(pointer.clone());
                pointers(child, pointer, out);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}

/// The indent of `line`, in bytes.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The line (from 0) the comment at `comment` (bytes of `text`, split in
/// `lines` starting at `starts`) belongs to, of the lines that write an
/// item (`items`, by line); `None` when no line writes one.
fn owner_line(
    (lines, starts): (&[&str], &[usize]),
    items: &BTreeMap<usize, Vec<&String>>,
    comment: &Range<usize>,
) -> Option<usize> {
    let line_of = |at: usize| starts.partition_point(|start| *start <= at) - 1;
    let first = line_of(comment.start);
    let last = line_of(comment.end.saturating_sub(1).max(comment.start));
    let before = &lines[first][..comment.start - starts[first]];
    if !before.trim().is_empty() {
        return Some(first);
    }
    let after_end = (comment.end - starts[last]).min(lines[last].len());
    if !lines[last][after_end..].trim().is_empty() {
        return Some(last);
    }
    let above = items.range(..first).next_back().map(|(line, _)| *line);
    match items.range(last + 1..).next().map(|(line, _)| *line) {
        Some(below) if indent(lines[first]) > indent(lines[below]) => above.or(Some(below)),
        Some(below) => Some(below),
        None => above,
    }
}

/// Whether the save (reading `intended`) keeps the item at `pointer` of
/// the host's last value `old`: where it was, or where the edit moved it
/// or one holding it (`moved`, by source pointer), a scalar holding what
/// it held, a map or list still one.
fn kept(
    pointer: &str,
    (old, intended): (&Value, &Value),
    moved: Option<&HashMap<&str, &str>>,
) -> bool {
    let Some(was) = old.pointer(pointer) else {
        return false;
    };
    // Where it is now, from the root down: each step where the edit moved
    // it (or a map or list holding it), else under the same key. A list
    // item the links do not tell is told by what it holds: an index
    // another item takes after a removal is not it.
    let mut now = String::new();
    let mut at = 0;
    while at < pointer.len() {
        let end = pointer[at + 1..]
            .find('/')
            .map_or(pointer.len(), |next| at + 1 + next);
        let (prefix, parent) = (&pointer[..end], &pointer[..at]);
        match moved.and_then(|moved| moved.get(prefix)) {
            Some(to) => now = (*to).to_string(),
            None => {
                now.push_str(&pointer[at..end]);
                let in_list = old.pointer(parent).is_some_and(Value::is_array);
                if in_list && intended.pointer(&now) != old.pointer(prefix) {
                    return false;
                }
            }
        }
        at = end;
    }
    match (was, intended.pointer(&now)) {
        (Value::Object(_), Some(Value::Object(_))) | (Value::Array(_), Some(Value::Array(_))) => {
            true
        }
        (Value::Object(_) | Value::Array(_), _) => false,
        (_, is) => is == Some(was),
    }
}

/// The first comment of the host's last text `previous` (reading `old`)
/// that belongs to a line the save leaves alone (see the module's rule)
/// and that `saved` (reading `intended`) no longer writes, unless the
/// user deleted it in a view that showed it (`deleted`, by its words):
/// the pointer of the line's first item, and the comment's words.
pub(super) fn first_lost(
    host: &dyn ConfigLanguage,
    (previous, saved): (&str, &str),
    (old, intended): (&Value, &Value),
    items: Option<&ItemMap>,
    deleted: &dyn Fn(&str) -> bool,
) -> Option<(String, String)> {
    let mut left = counted(host, saved);
    let comments = host.comments(previous);
    // Only comments the save no longer writes as often need an owner.
    let mut missing = Vec::new();
    for range in comments {
        let Some(comment) = previous.get(range.clone()) else {
            continue;
        };
        let words = words(host, comment);
        match left.get_mut(&words) {
            Some(count) if *count > 0 => *count -= 1,
            _ if words.is_empty() || deleted(&words) => {}
            _ => missing.push((range, words)),
        }
    }
    if missing.is_empty() {
        return None;
    }
    let mut all = Vec::new();
    pointers(old, &mut String::new(), &mut all);
    // A member of a container written over lines with brackets (an
    // item of a TOML or HCL list) is found in it.
    let found = |spans: Vec<Option<Span>>, key: bool| -> Vec<Option<Span>> {
        spans
            .into_iter()
            .zip(&all)
            .map(|(span, pointer)| {
                span.or_else(|| codec::inline_location(host, previous, pointer, key))
            })
            .collect()
    };
    let (keys, values) = paths::with_spans(host, previous, &all, || {
        (
            found(host.key_spans(previous, &all), true),
            found(host.value_spans(previous, &all), false),
        )
    });
    let mut on_line: BTreeMap<usize, Vec<&String>> = BTreeMap::new();
    for ((pointer, key), value) in all.iter().zip(keys).zip(values) {
        let mut lines: Vec<usize> = key
            .iter()
            .chain(value.iter())
            .filter_map(|span| span.line.checked_sub(1))
            .collect();
        // An item written as lines of its own (a TOML `[[s]]` table) is
        // written on its first one too.
        let in_list = pointer
            .rsplit_once('/')
            .and_then(|(parent, _)| old.pointer(parent))
            .is_some_and(Value::is_array);
        if in_list && let Some(own) = host.item_lines(previous, pointer) {
            lines.push(paths::line_of_byte(previous, own.start));
        }
        lines.sort_unstable();
        lines.dedup();
        for line in lines {
            on_line.entry(line).or_default().push(pointer);
        }
    }
    let lines: Vec<&str> = previous.split('\n').collect();
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0;
    for line in &lines {
        starts.push(at);
        at += line.len() + 1;
    }
    let moved: Option<HashMap<&str, &str>> = items.map(|items| {
        items
            .iter()
            .filter_map(|(to, source)| match source {
                ItemSource::Previous(from) => Some((from.as_str(), to.as_str())),
                ItemSource::Origin(_) => None,
            })
            .collect()
    });
    missing.into_iter().find_map(|(range, words)| {
        let line = owner_line((&lines, &starts), &on_line, &range)?;
        let items = on_line.get(&line)?;
        items
            .iter()
            .all(|pointer| kept(pointer, (old, intended), moved.as_ref()))
            .then(|| ((*items[0]).clone(), words))
    })
}

#[cfg(all(test, feature = "yaml", feature = "toml"))]
mod tests {
    use super::first_lost;
    use crate::editor::languages::Language;

    /// The comment `first_lost` names when `previous` (in `language`) is
    /// saved as `saved`.
    fn lost(language: Language, previous: &str, saved: &str) -> Option<(String, String)> {
        let old = language.parse(previous).unwrap();
        let new = language.parse(saved).unwrap();
        first_lost(
            language.behavior(),
            (previous, saved),
            (&old, &new),
            None,
            &|_| false,
        )
    }

    const YAML: &str = "# about a\na: 1\nb: 2 # about b\nc:\n  d: 1\n  # end of c\ne: 3\n";

    fn named(pointer: &str, words: &str) -> Option<(String, String)> {
        Some((pointer.to_string(), words.to_string()))
    }

    #[test]
    fn a_leading_comment_belongs_to_the_line_below_it() {
        let saved = YAML.replace("# about a\n", "");
        assert_eq!(lost(Language::YAML, YAML, &saved), named("/a", "about a"));
        // Its item changed: the comment was on an edited line.
        let saved = YAML.replace("# about a\na: 1", "a: 5");
        assert_eq!(lost(Language::YAML, YAML, &saved), None);
    }

    #[test]
    fn a_trailing_comment_belongs_to_its_own_line() {
        let saved = YAML.replace(" # about b", "");
        assert_eq!(lost(Language::YAML, YAML, &saved), named("/b", "about b"));
        let saved = YAML.replace("b: 2 # about b", "b: 3");
        assert_eq!(lost(Language::YAML, YAML, &saved), None);
    }

    #[test]
    fn a_comment_indented_deeper_than_the_next_line_ends_the_block_above() {
        let saved = YAML.replace("  # end of c\n", "");
        assert_eq!(
            lost(Language::YAML, YAML, &saved),
            named("/c/d", "end of c")
        );
        // `e` below changed, `d` above did not: still `d`'s.
        let saved = YAML.replace("  # end of c\ne: 3", "e: 4");
        assert_eq!(
            lost(Language::YAML, YAML, &saved),
            named("/c/d", "end of c")
        );
        let saved = YAML.replace("  d: 1\n  # end of c", "  d: 2");
        assert_eq!(lost(Language::YAML, YAML, &saved), None);
    }

    #[test]
    fn a_list_item_is_told_by_what_it_holds_not_its_index() {
        let toml = "# about a\n[[s]]\nname = \"a\"\n\n[[s]]\nname = \"b\"\n";
        // The first table removed: `b` takes its index, not its comment.
        assert_eq!(lost(Language::TOML, toml, "[[s]]\nname = \"b\"\n"), None);
        assert_eq!(
            lost(Language::TOML, toml, "[[s]]\nname = \"a\"\n"),
            named("/s", "about a")
        );
    }
}
