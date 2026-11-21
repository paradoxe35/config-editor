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
use super::super::language::{ConfigLanguage, Span, pointer_key};
use super::super::paths;
use super::ItemMap;
use crate::editor::paths::FieldPath;
use crate::editor::text::{TextRange, TextSize, checked_range, slice, text_size};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

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

/// The comments `text` writes (in `language`), each with its place: the
/// language's hook gives `usize` byte ranges.
fn comments<'t>(language: &dyn ConfigLanguage, text: &'t str) -> Vec<(TextRange, &'t str)> {
    language
        .comments(text)
        .iter()
        .filter_map(|range| {
            let range = checked_range(range)?;
            Some((range, slice::slice(text, range)?))
        })
        .collect()
}

/// The words of each comment `text` writes, with how many write them.
fn counted(language: &dyn ConfigLanguage, text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for (_, comment) in comments(language, text) {
        *counts.entry(words(language, comment)).or_insert(0) += 1;
    }
    counts
}

/// The comments `text` writes, by their words (see [`words`]).
pub(super) fn comment_words(language: &dyn ConfigLanguage, text: &str) -> Vec<String> {
    comments(language, text)
        .into_iter()
        .map(|(_, comment)| words(language, comment))
        .collect()
}

/// Every pointer of `value` (the root left out), parents first.
fn pointers(value: &Value, pointer: &mut String, out: &mut Vec<String>) {
    let length = pointer.len();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&pointer_key(key));
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
    (lines, starts): (&[&str], &[TextSize]),
    items: &BTreeMap<usize, Vec<&String>>,
    comment: TextRange,
) -> Option<usize> {
    let line_of = |at: TextSize| starts.partition_point(|start| *start <= at) - 1;
    let first = line_of(comment.start());
    let last_byte = comment
        .end()
        .checked_sub(TextSize::from(1))
        .unwrap_or_default();
    let last = line_of(last_byte.max(comment.start()));
    let before = slice::before(lines[first], comment.start() - starts[first]).unwrap_or_default();
    if !before.trim().is_empty() {
        return Some(first);
    }
    let after_end = (comment.end() - starts[last]).min(TextSize::of(lines[last]));
    if !slice::after(lines[last], after_end)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
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
fn kept(pointer: &str, (old, intended): (&Value, &Value), moved: Option<&Moves<'_>>) -> bool {
    let Some(was) = old.pointer(pointer) else {
        return false;
    };
    // Where it is now, from the root down: each step where the edit moved
    // it (or a map or list holding it), else under the same key. A list
    // item the links do not tell is told by what it holds: an index
    // another item takes after a removal is not it.
    let mut now = String::new();
    // Where each step of the pointer ends (at the next `/`).
    let ends = pointer
        .match_indices('/')
        .map(|(slash, _)| slash)
        .chain(std::iter::once(pointer.len()))
        .filter(|end| *end > 0);
    let mut at = 0;
    for end in ends {
        let (Some(prefix), Some(parent), Some(step)) =
            (pointer.get(..end), pointer.get(..at), pointer.get(at..end))
        else {
            return false;
        };
        match moved.and_then(|moved| moved.of(prefix)) {
            Some(to) => now = to.to_string(),
            None => {
                now.push_str(step);
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

/// What a save tells of its items' sources (see `links::Told`).
#[derive(Clone, Copy)]
pub(super) enum Owners<'a> {
    /// Each item's source, by its pointer in the save.
    Told(&'a ItemMap),
    /// Every item is the host's at its own pointer.
    InPlace,
}

/// Where each host item the save keeps is in the save, by its pointer in
/// the host's text.
enum Moves<'a> {
    Told(HashMap<&'a str, &'a str>),
    InPlace,
}

impl Moves<'_> {
    /// Where the host item at `pointer` is in the save, when told.
    fn of<'p>(&'p self, pointer: &'p str) -> Option<&'p str> {
        match self {
            Moves::Told(moved) => moved.get(pointer).copied(),
            Moves::InPlace => Some(pointer),
        }
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
    items: Option<Owners<'_>>,
    deleted: &dyn Fn(&str) -> bool,
) -> Option<(String, String)> {
    let mut left = counted(host, saved);
    // Only comments the save no longer writes as often need an owner.
    let mut missing = Vec::new();
    for (range, comment) in comments(host, previous) {
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
    // Every span and item's lines from one read of the text.
    let on_line: BTreeMap<usize, Vec<&String>> = paths::with_spans(host, previous, &all, || {
        let paths: Vec<FieldPath> = all.iter().map(FieldPath::from).collect();
        let keys = found(host.key_spans(previous, &paths), true);
        let values = found(host.value_spans(previous, &paths), false);
        let mut on_line: BTreeMap<usize, Vec<&String>> = BTreeMap::new();
        for ((pointer, key), value) in all.iter().zip(keys).zip(values) {
            let mut lines: Vec<usize> = key
                .iter()
                .chain(value.iter())
                .filter_map(|span| span.line.checked_sub(1))
                .collect();
            // An item written as lines of its own (a TOML `[[s]]` table)
            // is written on its first one too.
            let in_list = pointer
                .rsplit_once('/')
                .and_then(|(parent, _)| old.pointer(parent))
                .is_some_and(Value::is_array);
            if in_list
                && let Some(own) = host.item_lines(previous, &FieldPath::from(pointer.as_str()))
            {
                lines.push(paths::line_of_byte(previous, text_size(own.start)));
            }
            lines.sort_unstable();
            lines.dedup();
            for line in lines {
                on_line.entry(line).or_default().push(pointer);
            }
        }
        on_line
    });
    let lines: Vec<&str> = previous.split('\n').collect();
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = TextSize::default();
    for line in &lines {
        starts.push(at);
        at += TextSize::of(*line) + TextSize::from(1);
    }
    let moved: Option<Moves<'_>> = items.map(|items| match items {
        Owners::Told(items) => Moves::Told(
            items
                .iter()
                .filter_map(|(to, source)| match source {
                    ItemSource::Previous(from) => Some((from.as_str(), to.as_str())),
                    ItemSource::Origin(_) => None,
                })
                .collect(),
        ),
        Owners::InPlace => Moves::InPlace,
    });
    missing.into_iter().find_map(|(range, words)| {
        let line = owner_line((&lines, &starts), &on_line, range)?;
        let items = on_line.get(&line)?;
        items
            .iter()
            .all(|pointer| kept(pointer, (old, intended), moved.as_ref()))
            .then(|| ((*items[0]).clone(), words))
    })
}

/// The words of the first comment of the host's last text `previous` that
/// `saved` writes fewer times than the view's text `shown` (in `view`)
/// does: one the view keeps though its item went (its line left in the
/// view), or one pasted back in. The save would drop a comment on
/// screen; it waits instead.
pub(super) fn dropped_but_shown(
    host: &dyn ConfigLanguage,
    view: &dyn ConfigLanguage,
    (previous, saved): (&str, &str),
    shown: &str,
) -> Option<String> {
    let before = counted(host, previous);
    let after = counted(host, saved);
    let on_screen = counted(view, shown);
    let written = |words: &str| after.get(words).copied().unwrap_or(0);
    before
        .into_iter()
        .filter(|(words, count)| !words.is_empty() && written(words) < *count)
        .filter(|(words, _)| on_screen.get(words).copied().unwrap_or(0) > written(words))
        .map(|(words, _)| words)
        .min()
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
