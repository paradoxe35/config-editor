//! Every copy of some fields' keys in a text, in a few readings of it
//! however many copies there are: the places each key found is spelled
//! that name the same field, then a stand-in of the text with each of
//! them renamed apart (its letters changed for letters, a different name
//! for each place), where the language finds each copy's value under its
//! own name.

use std::collections::HashMap;
use std::ops::Range;

use super::super::super::codec::scalar_range;
use super::super::super::language::ConfigLanguage;
use super::super::super::paths::{
    TextPosition, key_range, paths_at, with_spans, written_value_span,
};
use super::super::super::work_count::{Work, note};
use super::Copies;

/// A place a field's key is spelled: its bytes, and the field.
type Place = (Range<usize>, String);

/// The value of each of `pointers` in every copy of its key.
pub(super) fn every_copy(behavior: &dyn ConfigLanguage, text: &str, pointers: &[String]) -> Copies {
    let mut copies = Copies::new();
    let mut places = with_spans(behavior, text, pointers, || {
        places(behavior, text, pointers)
    });
    // A stand-in holds as many as the names a spelling's letters make; the
    // rest go to the next (a key of one letter written many times).
    while !places.is_empty() {
        let (stand_in, renamed, rest) = renamed_apart(text, places);
        if renamed.is_empty() {
            break;
        }
        for (pointer, value) in values(behavior, &stand_in, &renamed) {
            copies.entry(pointer).or_default().push(value);
        }
        places = rest;
    }
    for ranges in copies.values_mut() {
        ranges.sort_by_key(|range| (range.start, range.end));
        ranges.dedup();
    }
    copies
}

/// The places the key found for each of `pointers` is spelled in `text`
/// where the language reads the same field (inside a batch for `text`).
fn places(behavior: &dyn ConfigLanguage, text: &str, pointers: &[String]) -> Vec<Place> {
    let starts = line_starts(text);
    let mut spellings: HashMap<&str, Vec<&String>> = HashMap::new();
    for pointer in pointers {
        let Some(key) = key_range(behavior, text, pointer) else {
            continue;
        };
        let bytes = byte_at(&starts, text, key.start)..byte_at(&starts, text, key.end);
        if let Some(spelling) = text
            .get(bytes)
            .filter(|spelling| spelling.bytes().any(|byte| byte.is_ascii_alphanumeric()))
        {
            spellings.entry(spelling).or_default().push(pointer);
        }
    }
    let mut written: Vec<(usize, &str)> = spellings
        .keys()
        .flat_map(|spelling| text.match_indices(spelling).map(|(at, _)| (at, *spelling)))
        .collect();
    written.sort_unstable();
    // Two spellings written over each other: the first is renamed.
    let mut end = 0;
    written.retain(|(at, spelling)| {
        let apart = *at >= end;
        if apart {
            end = at + spelling.len();
        }
        apart
    });
    let cursors = units_at(text, written.iter().map(|(at, _)| *at));
    written
        .into_iter()
        .zip(paths_at(behavior, text, &cursors))
        .filter_map(|((at, spelling), path)| {
            let pointer = path?.to_pointer();
            spellings[spelling]
                .iter()
                .any(|wanted| **wanted == pointer)
                .then(|| (at..at + spelling.len(), pointer))
        })
        .collect()
}

/// `text` with as many of `places` as fit renamed apart (each place's
/// letters and digits made a name of letters of its own, never its own
/// spelling), those renamed, and the rest.
fn renamed_apart(text: &str, places: Vec<Place>) -> (String, Vec<Place>, Vec<Place>) {
    let mut bytes = text.as_bytes().to_vec();
    // Names of each length: no two places share one.
    let mut names: HashMap<usize, usize> = HashMap::new();
    let (mut renamed, mut rest) = (Vec::new(), Vec::new());
    for place in places {
        let spelling = &text[place.0.clone()];
        let at: Vec<usize> = spelling
            .bytes()
            .enumerate()
            .filter(|(_, byte)| byte.is_ascii_alphanumeric())
            .map(|(index, _)| place.0.start + index)
            .collect();
        let own: Vec<u8> = at.iter().map(|at| text.as_bytes()[*at]).collect();
        let next = names.entry(at.len()).or_default();
        let name = loop {
            let name = letters(*next, at.len());
            *next += 1;
            match name {
                Some(name) if name == own => continue,
                name => break name,
            }
        };
        let Some(name) = name else {
            rest.push(place);
            continue;
        };
        for (at, letter) in at.iter().zip(name) {
            bytes[*at] = letter;
        }
        renamed.push(place);
    }
    // Letters for letters and digits: still UTF-8.
    let stand_in = String::from_utf8(bytes).unwrap_or_else(|_| text.to_string());
    (stand_in, renamed, rest)
}

/// The `index`th name of `length` lowercase letters; `None` past the last.
fn letters(mut index: usize, length: usize) -> Option<Vec<u8>> {
    let mut name = vec![b'a'; length];
    for letter in name.iter_mut().rev() {
        *letter = b'a' + (index % 26) as u8;
        index /= 26;
    }
    (index == 0).then_some(name)
}

/// The value written for each of `renamed` in `stand_in`: read under the
/// name the language gives its place, where that name's key is the one
/// written there.
fn values(
    behavior: &dyn ConfigLanguage,
    stand_in: &str,
    renamed: &[Place],
) -> Vec<(String, Range<usize>)> {
    note(Work::StandIn);
    let cursors = units_at(stand_in, renamed.iter().map(|(bytes, _)| bytes.start));
    let named: Vec<(String, &Place)> = paths_at(behavior, stand_in, &cursors)
        .into_iter()
        .zip(renamed)
        .filter_map(|(path, place)| Some((path?.to_pointer(), place)))
        .collect();
    let names: Vec<String> = named.iter().map(|(name, _)| name.clone()).collect();
    with_spans(behavior, stand_in, &names, || {
        let starts = line_starts(stand_in);
        named
            .iter()
            .filter_map(|(name, (bytes, pointer))| {
                let key = key_range(behavior, stand_in, name)?;
                (byte_at(&starts, stand_in, key.start) == bytes.start).then_some(())?;
                let value = written_value_span(behavior, stand_in, name)
                    .and_then(|span| scalar_range(behavior, stand_in, span))?;
                Some((pointer.clone(), value))
            })
            .collect()
    })
}

/// The UTF-16 offset of each of `bytes` (rising) of `text`.
fn units_at(text: &str, bytes: impl Iterator<Item = usize>) -> Vec<usize> {
    let (mut from, mut units) = (0, 0);
    bytes
        .map(|at| {
            units += text[from..at].encode_utf16().count();
            from = at;
            units
        })
        .collect()
}

/// The byte each line of `text` starts at, whatever ends the lines (the
/// positions the language gives count CR, LF and CRLF as line ends).
fn line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\r' if bytes.get(at + 1) == Some(&b'\n') => {
                at += 2;
                starts.push(at);
            }
            b'\r' | b'\n' => {
                at += 1;
                starts.push(at);
            }
            _ => at += 1,
        }
    }
    starts
}

/// The byte of `text` (whose lines start at `starts`) at one-based
/// `position` (its column in characters, clamped to the line).
fn byte_at(starts: &[usize], text: &str, position: TextPosition) -> usize {
    let start = starts[position.line.saturating_sub(1).min(starts.len() - 1)];
    let rest = &text[start..];
    let content = &rest[..rest.find(['\r', '\n']).unwrap_or(rest.len())];
    start
        + content
            .char_indices()
            .nth(position.column.saturating_sub(1))
            .map_or(content.len(), |(at, _)| at)
}
