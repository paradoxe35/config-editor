//! Secret placeholders: tokens such as `«secret 3f2a»` the host writes in
//! place of real secrets. A token is real only in the field the host gave
//! it in (the path whose value held it), at most as often as there: moved
//! into another field (pasted, or its key renamed) it is plain text and
//! flagged, so it can never stand for another field's secret; put back in
//! its own field it is real again, since the value is then what the host
//! gave. Each real token is atomic (the caret never lands inside one, and
//! an edit touching one replaces it whole). A token the host wrote in a
//! comment is real only in that text's comments: in a field it is plain
//! text and flagged, as a field's token is in a comment.
//!
//! A token is opaque: it is whatever the host's pattern matches, matched
//! and kept whole as written; nothing is read from inside it, and two
//! tokens are told apart only by their whole text and their place.

use std::ops::Range;

use leptos::prelude::Callable;
use leptos::prelude::Callback;

use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::paths::{api_units, field_key, in_batch, path_at, with_spans};
use super::super::work_count::{Work, note};
use super::glued::written_alone;
use super::marks::escape;
use super::unplaced::{in_open_comment, runs_open, written_in};
use crate::editor::text::{TextRange, TextSize, byte_range, slice, text_size};

/// A token of the host's text: the field it stands in (or the text's
/// comments), and how often that field's value (or the comments) hold it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Known {
    pub(crate) token: String,
    /// JSON pointer of the field (see `paths::same_field`); `None`: the token is
    /// written in the text's comments.
    pub(crate) field: Option<String>,
    pub(crate) count: usize,
    /// The host's field's value when it holds the token alone: the token,
    /// with nothing but whitespace around it (what a multi-line form
    /// writes: a block scalar's or heredoc's last line break, the
    /// indentation of XML text on its own line). There the token stands
    /// for the whole secret, and nothing may be written beside it (see
    /// `glued`).
    pub(crate) whole: Option<String>,
    /// The prints of the list items on the way to the field (see
    /// `follow_items`): the field follows its item when items shift.
    pub(crate) items: Vec<u64>,
    /// Whether `field` is where the host's text writes the token. Not for
    /// a token of a host text that does not parse whose field its reading
    /// cannot tell (an opener left unclosed above it, a list, a comment,
    /// carries the reading over it): it is taken as its line's whole value
    /// (the line kept while the text is broken, see `whole_guard`), and its
    /// field is read where it is once the text on screen parses (see
    /// `unplaced::placed_where_written`).
    pub(crate) placed: bool,
}

impl Known {
    /// Whether a field's `value` holds this token, given whole, alone: as
    /// the host wrote it, or the bare token (a block's chomping or a
    /// view's string form changed, the token kept).
    pub(crate) fn alone_in(&self, value: &str) -> bool {
        self.whole
            .as_deref()
            .is_some_and(|whole| value == whole || value == self.token)
    }
}

/// The pointer of the field holding the token at byte `start` of `text`.
fn field_at(behavior: &dyn ConfigLanguage, text: &str, start: TextSize) -> String {
    path_at(behavior, text, api_units(text, usize::from(start)))
        .map(|path| path.to_pointer())
        .unwrap_or_default()
}

/// Runs `work`, which looks up fields of `text` one by one, with the text
/// read once for all of them (thousands of placeholders in a text that
/// does not parse each read it from the top).
fn batched<T>(behavior: &dyn ConfigLanguage, text: &str, work: impl FnOnce() -> T) -> T {
    if in_batch(text) {
        work()
    } else {
        with_spans(behavior, text, &[], work)
    }
}

use super::super::secret_options::SecretOptions;

/// A text's hash and length, and the tokens found in it.
type Found = (u64, usize, Vec<Range<usize>>);

/// [`SecretOptions`] made ready to match.
pub(crate) struct SecretMatcher {
    pattern: Option<crate::editor::pattern_engine::Regex>,
    is_placeholder: Option<Callback<String, bool>>,
    /// The tokens of the last text searched whole: one edit's placement,
    /// check and problems each look for them in the same text, and the
    /// pattern is a read of all of it.
    last: std::sync::Mutex<Option<Found>>,
}

impl SecretMatcher {
    /// `None` pattern when it cannot be used (it does not compile, or
    /// matches empty text, see [`SecretOptions::check_placeholder`]): no
    /// token is ever real then.
    pub(crate) fn new(options: &SecretOptions) -> Self {
        SecretMatcher {
            pattern: options.check_placeholder().ok().and_then(|()| {
                crate::editor::pattern_engine::Regex::new(&options.placeholder).ok()
            }),
            is_placeholder: options.is_placeholder,
            last: std::sync::Mutex::new(None),
        }
    }

    /// Byte ranges of the tokens the pattern (and the host check) accept in
    /// the document `text`, from the last search when it was of the same
    /// text.
    pub(crate) fn find(&self, text: &str) -> Vec<Range<usize>> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let key = (hasher.finish(), text.len());
        let last = || {
            self.last
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        let remembered = last()
            .as_ref()
            .filter(|(hash, len, _)| (*hash, *len) == key)
            .map(|(.., found)| found.clone());
        if let Some(found) = remembered {
            return found;
        }
        // The host's check runs with nothing held.
        let found = self.find_in(text);
        *last() = Some((key.0, key.1, found.clone()));
        found
    }

    /// Whether `other` finds the same tokens in every text: the same
    /// pattern, and no host check (a check is a function, never compared).
    pub(crate) fn finds_as(&self, other: &SecretMatcher) -> bool {
        self.is_placeholder.is_none()
            && other.is_placeholder.is_none()
            && self.pattern.as_ref().map(|pattern| pattern.as_str())
                == other.pattern.as_ref().map(|pattern| pattern.as_str())
    }

    /// [`Self::find`] in a piece of text (a string of the parsed document),
    /// not remembered.
    pub(crate) fn find_in(&self, text: &str) -> Vec<Range<usize>> {
        let Some(pattern) = &self.pattern else {
            return Vec::new();
        };
        pattern
            .find_iter(text)
            .filter(|found| !found.as_str().is_empty())
            .filter(|found| {
                self.is_placeholder
                    .is_none_or(|check| check.run(found.as_str().to_string()))
            })
            .map(|found| found.range())
            .collect()
    }

    /// The tokens of the host's `text` (written in `behavior`), each once
    /// per field, with how often that field holds it.
    ///
    /// Read off the parsed document (one parse, whatever the number of
    /// tokens); a text that does not parse is read token by token instead,
    /// each field's value where the language finds it written (see
    /// [`written_alone`]). With `reading` (the schema), the document is
    /// read as the views read it: an XML list of one element is a list of
    /// one item, its token an item's whole value (`/l/0`).
    pub(crate) fn known(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        reading: Option<&serde_json::Value>,
    ) -> Vec<Known> {
        let mut known: Vec<Known> = Vec::new();
        // Where each token is in `known`, by its field: a lookup per held
        // token, not a pass over the fields already holding it (one token
        // held in thousands of fields).
        let mut by_place: std::collections::HashMap<(String, Option<String>, bool), usize> =
            std::collections::HashMap::new();
        let mut add =
            |token: &str, field: Option<String>, whole: Option<String>, items: Vec<u64>, placed| {
                note(Work::PlaceLookup);
                let place = (
                    token.to_string(),
                    field.as_deref().map(|field| field_key(behavior, field)),
                    placed,
                );
                match by_place.get(&place) {
                    Some(index) => known[*index].count += 1,
                    None => {
                        by_place.insert(place, known.len());
                        known.push(Known {
                            token: token.to_string(),
                            field,
                            count: 1,
                            whole,
                            items,
                            placed,
                        });
                    }
                }
            };
        let found = self.find(text);
        // The comments, read only when the text holds a token at all.
        let comments = if found.is_empty() {
            Vec::new()
        } else {
            behavior.comments(text)
        };
        let commented = |start: usize| in_comment(&comments, text_size(start));
        match super::super::codec::read(behavior, text, reading) {
            Ok(document) => {
                let mut printed = std::collections::HashMap::new();
                self.each_held(&document, |token, pointer, whole| {
                    let items =
                        super::follow_items::item_prints_in(&document, pointer, &mut printed);
                    let whole = whole.map(str::to_string);
                    add(token, Some(pointer.to_string()), whole, items, true);
                });
                for range in found.iter().filter(|range| commented(range.start)) {
                    add(
                        text.get(range.clone()).unwrap_or_default(),
                        None,
                        None,
                        Vec::new(),
                        true,
                    );
                }
            }
            Err(error) => {
                // Read past where the text stops parsing, a token's field
                // may be read wrong (an opener left unclosed before it):
                // checked against where its value is written.
                let broken_at = error.line.map(|line| (line, error.column.unwrap_or(1)));
                let past_error = |start: usize| {
                    broken_at.is_some_and(|at| {
                        at <= super::super::paths::line_column(text, text_size(start))
                    })
                };
                // A construct over lines left open (a list, a character
                // data section) runs over the rest of the text, the token
                // in it.
                let constructs: Vec<Range<usize>> = behavior
                    .multiline_constructs(text)
                    .into_iter()
                    .map(|construct| construct.range)
                    .collect();
                let in_open_construct = |start: usize| {
                    constructs
                        .iter()
                        .any(|range| runs_open(text, range, text_size(start)))
                };
                // A comment left open runs over the rest of the text: what
                // it holds is no comment's, read as the text goes on.
                let commented = |start: usize| {
                    commented(start) && !in_open_comment(text, &comments, text_size(start))
                };
                // Each token's field from one reading of the text, then
                // the values of those fields read at once.
                batched(behavior, text, || {
                    let placed: Vec<(Range<usize>, String)> = found
                        .iter()
                        .filter(|range| !commented(range.start))
                        .map(|range| {
                            (
                                range.clone(),
                                field_at(behavior, text, text_size(range.start)),
                            )
                        })
                        .collect();
                    let mut pointers: Vec<String> =
                        placed.iter().map(|(_, pointer)| pointer.clone()).collect();
                    pointers.sort_unstable();
                    pointers.dedup();
                    with_spans(behavior, text, &pointers, || {
                        for (range, pointer) in placed {
                            let token = text.get(range.clone()).unwrap_or_default();
                            // Not where the reading puts its field's value:
                            // the reading went wrong before it.
                            if past_error(range.start)
                                || in_open_construct(range.start)
                                || !written_in(behavior, text, &pointer, &range)
                            {
                                let whole = Some(token.to_string());
                                add(token, Some(pointer), whole, Vec::new(), false);
                                continue;
                            }
                            let whole = written_alone(behavior, text, &pointer, range.clone());
                            add(token, Some(pointer), whole, Vec::new(), true);
                        }
                    });
                });
                for range in found.iter().filter(|range| commented(range.start)) {
                    add(
                        text.get(range.clone()).unwrap_or_default(),
                        None,
                        None,
                        Vec::new(),
                        true,
                    );
                }
            }
        }
        known
    }

    /// The pointers of the fields whose strings hold each token of
    /// `document`, once per time held.
    pub(crate) fn holders(
        &self,
        document: &serde_json::Value,
    ) -> std::collections::HashMap<String, Vec<String>> {
        let mut holders: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        self.each_held(document, |token, pointer, _| {
            holders
                .entry(token.to_string())
                .or_default()
                .push(pointer.to_string());
        });
        holders
    }

    /// Calls `found` with each token the strings of `document` hold, the
    /// pointer of the field holding it and, when the token is that field's
    /// whole value (with nothing but whitespace around it, see
    /// [`Known::whole`]), the value, in document order.
    fn each_held(
        &self,
        document: &serde_json::Value,
        mut found: impl FnMut(&str, &str, Option<&str>),
    ) {
        let mut stack = vec![(String::new(), document)];
        while let Some((pointer, value)) = stack.pop() {
            match value {
                serde_json::Value::String(written) => {
                    for range in self.find_in(written) {
                        let token = written.get(range).unwrap_or_default();
                        let whole = (written.trim() == token).then_some(written.as_str());
                        found(token, &pointer, whole);
                    }
                }
                serde_json::Value::Array(items) => stack.extend(
                    items
                        .iter()
                        .enumerate()
                        .rev()
                        .map(|(index, item)| (format!("{pointer}/{index}"), item)),
                ),
                serde_json::Value::Object(map) => stack.extend(
                    map.iter()
                        .rev()
                        .map(|(key, item)| (format!("{pointer}/{}", escape(key)), item)),
                ),
                _ => {}
            }
        }
    }
}

/// The ranges of the known tokens written in `text`, each as often as
/// the host wrote it, first ones first, without asking where each is (a
/// text replaced whole: an undo, a switch); the pause in typing then checks
/// they are in their fields (`EditorState::recheck_tokens`). One pass of the
/// pattern, however many tokens there are.
pub(crate) fn locate_written(
    matcher: &SecretMatcher,
    text: &str,
    known: &[Known],
) -> Vec<TextRange> {
    let mut left = written_counts(known);
    let mut found = Vec::new();
    for range in matcher.find(text) {
        let range = byte_range(range);
        let Some(count) = slice::slice(text, range)
            .and_then(|token| left.get_mut(token))
            .filter(|count| **count > 0)
        else {
            continue;
        };
        *count -= 1;
        found.push(range);
    }
    found
}

/// How often the host wrote each known token, over all its fields.
pub(crate) fn written_counts(known: &[Known]) -> std::collections::HashMap<&str, usize> {
    let mut counts = std::collections::HashMap::new();
    for known in known {
        *counts.entry(known.token.as_str()).or_default() += known.count;
    }
    counts
}

/// The value of the host's `field` in `document` (a text in `behavior`
/// read), also where a language that repeats keys reads a lone item of a
/// repeated field as the item (`/l/0` read `/l`, see `paths::field_key`).
pub(crate) fn field_value<'a>(
    behavior: &dyn ConfigLanguage,
    document: &'a serde_json::Value,
    field: &str,
) -> Option<&'a serde_json::Value> {
    document
        .pointer(field)
        .or_else(|| document.pointer(&field_key(behavior, field)))
}

/// How many times the parsed `document` holds each known token in the
/// fields the host gave it in, at most as often as there.
pub(crate) fn held_in_fields<'a>(
    behavior: &dyn ConfigLanguage,
    document: &serde_json::Value,
    known: &'a [Known],
) -> std::collections::HashMap<&'a str, usize> {
    let mut held = std::collections::HashMap::new();
    for known in known {
        let Some(field) = &known.field else {
            continue;
        };
        let count = field_value(behavior, document, field)
            .and_then(serde_json::Value::as_str)
            .map_or(0, |value| {
                value.matches(known.token.as_str()).count().min(known.count)
            });
        *held.entry(known.token.as_str()).or_default() += count;
    }
    held
}

impl SecretMatcher {
    /// The ranges of the real tokens in `text` (written in `behavior`):
    /// each `known` token in its own field, at most as often as the host
    /// wrote it there, first ones first.
    ///
    /// One parse says which field holds each occurrence (the document's
    /// strings hold a token in the order it is written); only a token the
    /// parse cannot place (a text that does not parse, a token also in a
    /// comment or a key) is looked up by path, occurrence by occurrence.
    pub(crate) fn locate(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        known: &[Known],
    ) -> Vec<TextRange> {
        batched(behavior, text, || self.locate_in(behavior, text, known))
    }

    /// [`Self::locate`], inside a batch for `text`.
    fn locate_in(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        known: &[Known],
    ) -> Vec<TextRange> {
        let holders = parse_cache::parsed(behavior, text)
            .as_ref()
            .as_ref()
            .map(|document| self.holders(document))
            .ok();
        let mut fields: std::collections::HashMap<&str, Vec<(Option<&str>, usize)>> =
            std::collections::HashMap::new();
        for known in known {
            fields
                .entry(known.token.as_str())
                .or_default()
                .push((known.field.as_deref(), known.count));
        }
        // Where each known token is written (one pass of the pattern).
        let mut written: std::collections::HashMap<&str, Vec<usize>> =
            std::collections::HashMap::new();
        for range in self.find(text) {
            if let Some((token, _)) =
                fields.get_key_value(text.get(range.clone()).unwrap_or_default())
            {
                written.entry(*token).or_default().push(range.start);
            }
        }
        // As the pattern gives them, typed at the end.
        let mut found: Vec<Range<usize>> = Vec::new();
        // The text's comments, read once if a token needs them.
        let mut comments: Option<Vec<Range<usize>>> = None;
        for (token, left) in &mut fields {
            let mut written = written.remove(*token).unwrap_or_default();
            // One the host wrote in a comment: real in the comments, as
            // often as there, first ones first.
            if let Some((_, count)) = left.iter_mut().find(|(field, _)| field.is_none()) {
                let comments = comments.get_or_insert_with(|| behavior.comments(text));
                let (commented, outside): (Vec<usize>, Vec<usize>) = written
                    .iter()
                    .partition(|byte| in_comment(comments, text_size(**byte)));
                for byte in commented.into_iter().take(*count) {
                    found.push(byte..byte + token.len());
                }
                *count = 0;
                written = outside;
            }
            // A document whose strings hold the token nowhere (each written
            // in a comment: a document commented out) has none in a field.
            let held = holders.as_ref().map(|holders| holders.get(*token));
            if held == Some(None) {
                continue;
            }
            let mut placed = held
                .flatten()
                .filter(|pointers| pointers.len() == written.len());
            // Written more often than held: those in comments (the document
            // pasted again commented out) are in no field; the others are
            // placed in order when that leaves as many as held.
            if let Some(pointers) = held.flatten()
                && placed.is_none()
            {
                let comments = comments.get_or_insert_with(|| behavior.comments(text));
                let outside: Vec<usize> = written
                    .iter()
                    .copied()
                    .filter(|byte| !in_comment(comments, text_size(*byte)))
                    .collect();
                if outside.len() == pointers.len() {
                    written = outside;
                    placed = Some(pointers);
                }
            }
            // The fields still to place this token in, by field: a lookup
            // per occurrence.
            let mut by_field: std::collections::HashMap<String, std::collections::VecDeque<usize>> =
                std::collections::HashMap::new();
            for (index, (field, _)) in left.iter().enumerate() {
                if let Some(field) = field {
                    by_field
                        .entry(field_key(behavior, field))
                        .or_default()
                        .push_back(index);
                }
            }
            for (nth, byte) in written.into_iter().enumerate() {
                let pointer = match placed {
                    Some(pointers) => pointers[nth].clone(),
                    None => field_at(behavior, text, text_size(byte)),
                };
                let Some(slots) = by_field.get_mut(&field_key(behavior, &pointer)) else {
                    continue;
                };
                note(Work::PlaceLookup);
                while slots.front().is_some_and(|index| left[*index].1 == 0) {
                    note(Work::PlaceLookup);
                    slots.pop_front();
                }
                let Some(&index) = slots.front() else {
                    continue;
                };
                left[index].1 -= 1;
                found.push(byte..byte + token.len());
            }
        }
        found.sort_by_key(|range| range.start);
        found.dedup_by(|later, earlier| later.start < earlier.end);
        found.into_iter().map(byte_range).collect()
    }
}

/// Whether byte `at` is inside one of `comments` (in order, apart).
pub(crate) fn in_comment(comments: &[Range<usize>], at: TextSize) -> bool {
    let at = usize::from(at);
    let next = comments.partition_point(|comment| comment.end <= at);
    comments
        .get(next)
        .is_some_and(|comment| comment.start <= at)
}

/// The edit turning `before` into `after` (whose replaced region is
/// `region` of `before`), widened so it never cuts a token: a token it
/// touches goes whole. `None` when it touches none.
pub(crate) fn widen(
    tokens: &[TextRange],
    before: &str,
    after: &str,
    region: TextRange,
) -> Option<(String, TextRange, TextSize)> {
    let inserted_len = (TextSize::of(after) + region.len()).checked_sub(TextSize::of(before))?;
    // Whole tokens written again (lines swapped or copied): no cut.
    if super::rewritten::rewritten_region(tokens, before, after, &region).is_some() {
        return None;
    }
    let mut wide = region;
    loop {
        let grown = tokens.iter().fold(wide, |wide, token| {
            if cuts(token, &wide) {
                wide.cover(*token)
            } else {
                wide
            }
        });
        if grown == wide {
            break;
        }
        wide = grown;
    }
    if wide == region {
        return None;
    }
    let inserted = slice::slice(after, TextRange::at(region.start(), inserted_len))?;
    Some((
        format!(
            "{}{inserted}{}",
            slice::before(before, wide.start())?,
            slice::after(before, wide.end())?
        ),
        wide,
        wide.start() + inserted_len,
    ))
}

/// Whether an edit replacing `region` cuts `token`: it removes part of it,
/// or inserts inside it.
pub(super) fn cuts(token: &TextRange, region: &TextRange) -> bool {
    if region.is_empty() {
        return token.start() < region.start() && region.start() < token.end();
    }
    let overlaps = region.start() < token.end() && token.start() < region.end();
    overlaps && !region.contains_range(*token)
}

/// The tokens after an edit replaced `region` (of the old text) with
/// `inserted_len` bytes: those before stay, those after shift, those it
/// touched are gone.
pub(crate) fn follow(
    tokens: &[TextRange],
    region: &TextRange,
    inserted_len: TextSize,
) -> Vec<TextRange> {
    tokens
        .iter()
        .filter_map(|token| {
            if token.end() <= region.start() {
                Some(*token)
            } else if token.start() >= region.end() {
                // Past the region: as far past what the edit wrote.
                Some(TextRange::at(
                    token.start() + inserted_len - region.len(),
                    token.len(),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Byte `offset` moved out of any token: to the edge it was moving
/// towards (`forward`), or the nearest one.
pub(crate) fn snap(tokens: &[TextRange], offset: TextSize, forward: Option<bool>) -> TextSize {
    let Some(token) = tokens
        .iter()
        .find(|token| token.start() < offset && offset < token.end())
    else {
        return offset;
    };
    match forward {
        Some(true) => token.end(),
        Some(false) => token.start(),
        None if offset - token.start() <= token.end() - offset => token.start(),
        None => token.end(),
    }
}

/// A selection grown to take whole every token it cuts.
pub(crate) fn snap_selection(
    tokens: &[TextRange],
    start: TextSize,
    end: TextSize,
) -> (TextSize, TextSize) {
    if start == end {
        let at = snap(tokens, start, None);
        return (at, at);
    }
    (
        snap(tokens, start, Some(false)),
        snap(tokens, end, Some(true)),
    )
}

#[cfg(test)]
#[allow(clippy::string_slice, reason = "the tests cut their own texts")]
mod tests {
    use super::*;

    const TOKEN: &str = "«secret 3f2a»";

    fn r(start: u32, end: u32) -> TextRange {
        TextRange::new(start.into(), end.into())
    }

    #[cfg(all(feature = "yaml", feature = "json"))]
    #[test]
    fn only_host_tokens_are_real_in_their_own_field_and_at_most_as_often() {
        use super::super::super::Language;
        let matcher = SecretMatcher::new(&SecretOptions::default());
        let host = format!("a: {TOKEN}\nb: «secret zz»\n");
        let yaml = Language::YAML.behavior();
        let known = matcher.known(yaml, &host, None);
        assert_eq!(
            known,
            vec![Known {
                token: TOKEN.to_string(),
                field: Some("/a".to_string()),
                count: 1,
                whole: Some(TOKEN.to_string()),
                items: Vec::new(),
                placed: true,
            }]
        );
        // Shown as JSON: real in `a`, plain in `c`.
        let shown = format!("{{\"c\": \"{TOKEN}\", \"a\": \"{TOKEN}\"}}");
        let start = shown.rfind(TOKEN).map(text_size).unwrap();
        let json = Language::JSON.behavior();
        assert_eq!(
            matcher.locate(json, &shown, &known),
            vec![TextRange::at(start, TextSize::of(TOKEN))]
        );
        let custom = SecretMatcher::new(
            &SecretOptions::default().with_matcher(|token| token.ends_with("2a»")),
        );
        assert_eq!(
            custom
                .known(yaml, "x: «secret 11»\ny: «secret 2a»", None)
                .len(),
            1
        );
        use crate::editor::paths::same_field;
        let (xml, json) = (Language::XML.behavior(), Language::JSON.behavior());
        assert!(same_field(xml, "/a/0/b", "/a/b") && !same_field(xml, "/a", "/b"));
        // An index is only an index in a language that never repeats a key.
        assert!(!same_field(json, "/a/0/b", "/a/b"));
    }

    #[test]
    fn edits_touching_a_token_take_it_whole() {
        let text = format!("k: {TOKEN} x");
        // `«` and `»` are two bytes each.
        let token = r(3, 18);
        let tokens = [token];
        // Backspace after it, Delete before it.
        let backspace = format!("k: {} x", &TOKEN[..TOKEN.len() - '»'.len_utf8()]);
        assert_eq!(
            widen(&tokens, &text, &backspace, r(16, 18)),
            Some(("k:  x".to_string(), token, 3.into()))
        );
        let delete = format!("k: {} x", &TOKEN['«'.len_utf8()..]);
        assert_eq!(
            widen(&tokens, &text, &delete, r(3, 5)).map(|(text, ..)| text),
            Some("k:  x".to_string())
        );
        // Typing over a selection that ends inside it (after `«s`).
        let typed = format!("k:Z{}", &text[7..]);
        assert_eq!(
            widen(&tokens, &text, &typed, r(2, 7)).map(|(text, ..)| text),
            Some("k:Z x".to_string())
        );
        // Typing at its edges, or deleting it whole, cuts nothing.
        assert_eq!(
            widen(&tokens, &text, &format!("k: Q{TOKEN} x"), r(3, 3)),
            None
        );
        assert_eq!(
            widen(&tokens, &text, &format!("k: {TOKEN}Q x"), r(18, 18)),
            None
        );
        assert_eq!(widen(&tokens, &text, "k:  x", r(3, 18)), None);
    }

    #[test]
    fn the_caret_snaps_out_of_tokens() {
        let tokens = vec![r(3, 16), r(20, 33)];
        let at = TextSize::from;
        assert_eq!(snap(&tokens, at(5), None), at(3));
        assert_eq!(snap(&tokens, at(14), None), at(16));
        assert_eq!(snap(&tokens, at(5), Some(true)), at(16));
        assert_eq!(snap(&tokens, at(3), Some(true)), at(3));
        assert_eq!(snap_selection(&tokens, at(5), at(25)), (at(3), at(33)));
    }
}
