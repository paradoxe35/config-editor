//! Which scalar holds each placeholder, read off where the placeholder is
//! written: of the strings the parse holds the token's text in, the one
//! whose written value (as the language finds it, through the value-span
//! hooks every language has) holds the placeholder's bytes. A placeholder
//! is an atomic span of the text; its holder is a fact of the text, so no
//! pointer is walked or guessed through the shapes a language makes of
//! repeats (an XML element written twice, its parent written twice, both:
//! a list in a list; an attribute's element copied). The rules on
//! placeholders ask this one query.
//!
//! One read of a text answers for all its placeholders: the document is
//! walked once for the strings holding one, their values' spans are read
//! in one batch (thousands of fields holding one fixed mask cost a pass,
//! never a pass each).

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use serde_json::Value;

use super::super::codec::scalar_range;
use super::super::language::{ConfigLanguage, pointer_key};
use super::super::paths::{path_at, with_spans, written_value_span};
use super::super::text_change::utf16_len;
use super::super::work_count::{Work, note};
use super::tokens::{SecretMatcher, field_value};

/// The scalar holding a placeholder: its pointer in the document, and its
/// text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Holder<'d> {
    pub(crate) pointer: String,
    pub(crate) text: &'d str,
}

/// The placeholders written in a text, each with its holder.
pub(crate) struct Holders<'d> {
    /// Each placeholder written, in the text's order: its bytes, and the
    /// string holding it there (`None`: no string holds it there, as in a
    /// comment, or the language cannot say where the value is written).
    written: Vec<(Range<usize>, Option<Holder<'d>>)>,
    /// Where each token's text is written, as positions in `written`.
    by_token: HashMap<String, Vec<usize>>,
}

/// Strings holding a token's text, with their pointers.
type Candidates<'d> = HashMap<String, Vec<(String, &'d str)>>;

/// A token's strings with where each is written.
type Placed<'c, 'd> = Vec<(Range<usize>, &'c (String, &'d str))>;

impl<'d> Holders<'d> {
    /// The placeholders `matcher` finds in `text` (which parses as
    /// `document`, read in `behavior`), each with the string holding its
    /// text whose written value holds its bytes, the innermost when
    /// several do.
    pub(crate) fn read(
        behavior: &dyn ConfigLanguage,
        text: &str,
        document: &'d Value,
        matcher: &SecretMatcher,
    ) -> Self {
        Self::found_by(behavior, text, document, &|text| matcher.find_in(text))
    }

    /// [`Self::read`] with the placeholders `find` finds (the byte ranges
    /// of those of a text): a problem message's patterns, which find every
    /// document's.
    pub(crate) fn found_by(
        behavior: &dyn ConfigLanguage,
        text: &str,
        document: &'d Value,
        find: &dyn Fn(&str) -> Vec<Range<usize>>,
    ) -> Self {
        let found = find(text);
        let mut holders = Self {
            written: Vec::with_capacity(found.len()),
            by_token: HashMap::new(),
        };
        if found.is_empty() {
            return holders;
        }
        note(Work::TokenScan);
        note(Work::HolderRead);
        let tokens: Vec<&str> = found
            .iter()
            .map(|range| text.get(range.clone()).unwrap_or_default())
            .collect();
        let wanted: HashSet<&str> = tokens.iter().copied().collect();
        let mut candidates: Candidates<'d> = HashMap::new();
        let found_in = |string: &str| -> Vec<String> {
            find(string)
                .into_iter()
                .filter_map(|range| string.get(range))
                .filter(|token| wanted.contains(token))
                .map(str::to_string)
                .collect()
        };
        strings_holding(document, String::new(), &found_in, &mut candidates);
        let pointers: Vec<String> = candidates
            .values()
            .flatten()
            .map(|(pointer, _)| pointer.clone())
            .collect();
        with_spans(behavior, text, &pointers, || {
            // Each token's strings where they are written, by start: a
            // placeholder is held by one written around it.
            let placed: HashMap<&str, Placed<'_, 'd>> = candidates
                .iter()
                .map(|(token, strings)| {
                    let mut at: Vec<_> = strings
                        .iter()
                        .filter_map(|string| {
                            written_value_span(behavior, text, &string.0)
                                .and_then(|span| scalar_range(behavior, text, span))
                                .map(|range| (range, string))
                        })
                        .collect();
                    at.sort_by_key(|(range, _)| (range.start, std::cmp::Reverse(range.end)));
                    (token.as_str(), at)
                })
                .collect();
            // The innermost string written around each placeholder (the
            // latest start), where one is.
            let by_span: Vec<Option<&(String, &'d str)>> = found
                .iter()
                .zip(&tokens)
                .map(|(range, token)| {
                    let at = placed.get(token)?;
                    let before = at.partition_point(|(written, _)| written.start <= range.start);
                    at[..before]
                        .iter()
                        .rev()
                        .find(|(written, _)| range.end <= written.end)
                        .map(|(_, string)| *string)
                })
                .collect();
            // The strings a copy of each token is placed in by its span:
            // never the holder of a copy no span is around.
            let taken: HashSet<(&str, &str)> = tokens
                .iter()
                .zip(&by_span)
                .filter_map(|(token, spanned)| Some((*token, spanned.as_ref()?.0.as_str())))
                .collect();
            for ((range, token), spanned) in found.iter().zip(&tokens).zip(by_span) {
                let holder = spanned
                    .map(|(pointer, value)| Holder {
                        pointer: pointer.clone(),
                        text: value,
                    })
                    .or_else(|| {
                        let strings = candidates.get(*token).map_or(&[][..], Vec::as_slice);
                        along_path(behavior, (text, document), range, token, strings, &taken)
                    });
                holders
                    .by_token
                    .entry((*token).to_string())
                    .or_default()
                    .push(holders.written.len());
                holders.written.push((range.clone(), holder));
            }
        });
        holders
    }

    /// The holders of each place `token` is written, in the text's order.
    pub(crate) fn of(&self, token: &str) -> impl Iterator<Item = &Option<Holder<'d>>> + '_ {
        self.by_token
            .get(token)
            .into_iter()
            .flatten()
            .map(|at| &self.written[*at].1)
    }

    /// The texts of the strings holding a placeholder.
    pub(crate) fn texts(&self) -> impl Iterator<Item = &'d str> + '_ {
        self.written
            .iter()
            .filter_map(|(_, holder)| holder.as_ref().map(|holder| holder.text))
    }

    /// The holder of the placeholder written at bytes `range`, if one is.
    pub(crate) fn at(&self, range: &Range<usize>) -> Option<&Holder<'d>> {
        let at = self
            .written
            .binary_search_by_key(&range.start, |(written, _)| written.start)
            .ok()?;
        let (written, holder) = &self.written[at];
        (written == range).then_some(holder.as_ref()).flatten()
    }
}

/// The holder of the placeholder at bytes `range` of `text` where no
/// value's span is found around it (a value written over lines a language
/// reads as one scalar, a declared one-line map, a broken line a parse
/// reads into the value of a key above it; `text` parses as `document`):
/// the string at the path the language reads at its place, else one of
/// `strings` (the document's strings holding `token`) inside the value
/// there, the path cut back until one is (that path may name a field the
/// parse does not have: a list's items where the parse makes them one
/// scalar). The one at the path is looked up, never searched for among
/// `strings` (thousands of copies of one mask cost no pass each). Only a
/// string no copy of the token is placed in by its span (`taken`) is
/// taken, at the path or inside: one placed is another copy's (a field's,
/// when the token is written in a comment; the last of a list written
/// twice under one key, which the parse keeps, when the token is in the
/// first). Inside, one with text beside it first: a token some free string
/// holds always has a holder, so a reading that cannot place it never
/// hides a glued one.
fn along_path<'d>(
    behavior: &dyn ConfigLanguage,
    (text, document): (&str, &'d Value),
    range: &Range<usize>,
    token: &str,
    strings: &[(String, &'d str)],
    taken: &HashSet<(&str, &str)>,
) -> Option<Holder<'d>> {
    if strings.is_empty() {
        return None;
    }
    let units = utf16_len(text.get(..range.start).unwrap_or_default());
    let mut pointer =
        path_at(behavior, text, units).map_or_else(String::new, |path| path.to_pointer());
    loop {
        // The string at the path: a lookup, never a pass of the strings.
        if let Some(Value::String(value)) = field_value(document, &pointer)
            && value.contains(token)
            && !taken.contains(&(token, pointer.as_str()))
        {
            return Some(Holder {
                pointer,
                text: value,
            });
        }
        let inside = |at: &str| {
            !taken.contains(&(token, at))
                && at
                    .strip_prefix(pointer.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        };
        let mut within = strings.iter().filter(|(at, _)| inside(at));
        let first = within.next();
        if let Some((at, value)) = first
            .filter(|(_, value)| value.trim() != token)
            .or_else(|| within.find(|(_, value)| value.trim() != token))
            .or(first)
        {
            return Some(Holder {
                pointer: at.clone(),
                text: value,
            });
        }
        if pointer.is_empty() {
            return None;
        }
        pointer.truncate(pointer.rfind('/').unwrap_or(0));
    }
}

/// The strings inside `value` (at `pointer`) holding a placeholder (the
/// tokens `found_in` finds in a string), with their pointers, by token.
fn strings_holding<'v>(
    value: &'v Value,
    pointer: String,
    found_in: &dyn Fn(&str) -> Vec<String>,
    found: &mut Candidates<'v>,
) {
    match value {
        Value::String(text) => {
            let mut tokens = found_in(text);
            tokens.sort_unstable();
            tokens.dedup();
            for token in tokens {
                found
                    .entry(token)
                    .or_default()
                    .push((pointer.clone(), text));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                strings_holding(item, format!("{pointer}/{index}"), found_in, found);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                strings_holding(
                    item,
                    format!("{pointer}/{}", pointer_key(key)),
                    found_in,
                    found,
                );
            }
        }
        _ => {}
    }
}
