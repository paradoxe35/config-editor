//! The value a token is written in when its field's key is written more
//! than once (a line moved, cut and pasted, or typed into a block that
//! holds the same key: the text does not parse). The language finds one of
//! them by the field's path; which one it finds is its own affair, so a
//! token in the other one has no value written for it, and was taken as
//! plain text (typing beside it reached the host glued) or kept its whole
//! line (no way back). Each other copy is found the same way in a stand-in
//! of the text with the key found renamed (one character changed for
//! another of the same length, so every byte stays where it is): each
//! token is judged in the value it is written in.
//!
//! A check judging many placeholders at once (an edit of the whole text)
//! finds the copies of all their fields together ([`with_own_values`]):
//! each stand-in renames the key found for every field, and is read once
//! for all of them, so the work is a few readings of the text, never a few
//! for each placeholder (QA 34: Ctrl+A Tab on the large sample pasted four
//! times read the text thousands of times and hung the page).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use super::super::codec::scalar_range;
use super::super::language::ConfigLanguage;
use super::super::paths::{TextPosition, key_range, with_spans, written_value_span};

/// Copies of one key a token is looked for among (a line pasted a few
/// times over); past them it has no value written for it.
const MAX_COPIES: usize = 4;

/// The value ranges written for each field, one per copy of its key, in
/// the order the stand-ins found them.
type Copies = HashMap<String, Vec<Range<usize>>>;

/// A text by address and length, and its language: borrowed while
/// [`with_own_values`] runs for it.
type TextKey = (usize, usize, &'static str);

thread_local! {
    static FOUND: RefCell<Vec<(TextKey, Rc<Copies>)>> = const { RefCell::new(Vec::new()) };
}

fn text_key(behavior: &dyn ConfigLanguage, text: &str) -> TextKey {
    (text.as_ptr() as usize, text.len(), behavior.info().label)
}

/// Runs `work` with the values written for each of `pointers` in `text`,
/// every copy of their keys, found at once: [`own_value`] calls for those
/// fields of that text inside it answer from them.
pub(crate) fn with_own_values<T>(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointers: &[String],
    work: impl FnOnce() -> T,
) -> T {
    let copies = Rc::new(copies(behavior, text, pointers, None));
    FOUND.with(|found| found.borrow_mut().push((text_key(behavior, text), copies)));
    // Taken off on the way out, a panic included.
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            let _ = FOUND.try_with(|found| found.borrow_mut().pop());
        }
    }
    let _pop = Pop;
    work()
}

/// The bytes of the value written for the field at `pointer` that holds
/// the token at bytes `token` of `text`: the first copy of the field's key
/// the language finds, or one written after it. (The stand-ins only
/// rename keys: the value's bytes read the same in `text`.)
pub(crate) fn own_value(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    token: &Range<usize>,
) -> Option<Range<usize>> {
    let holds = |range: &&Range<usize>| range.start <= token.start && token.end <= range.end;
    let key = text_key(behavior, text);
    let known = FOUND.with(|found| {
        found
            .borrow()
            .iter()
            .rev()
            .filter(|(text, _)| *text == key)
            .find_map(|(_, copies)| {
                copies
                    .get(pointer)
                    .map(|ranges| ranges.iter().find(holds).cloned())
            })
    });
    match known {
        Some(range) => range,
        None => copies(behavior, text, &[pointer.to_string()], Some(token))
            .get(pointer)?
            .iter()
            .find(holds)
            .cloned(),
    }
}

/// The values written for each of `pointers` in `text`, one per copy of
/// its key: each stand-in of the text read once for all of them (a single
/// field's lookups answer from the reading its caller holds). With
/// `wanted`, the search stops at the value holding those bytes.
fn copies(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointers: &[String],
    wanted: Option<&Range<usize>>,
) -> Copies {
    let mut found = Copies::new();
    // Made once a copy is renamed: most tokens are in the first.
    let mut starts: Option<Vec<usize>> = None;
    let mut probe: Option<String> = None;
    let mut active: Vec<String> = pointers.to_vec();
    active.sort_unstable();
    active.dedup();
    for pointer in &active {
        found.insert(pointer.clone(), Vec::new());
    }
    for round in 0..MAX_COPIES {
        let last = round + 1 == MAX_COPIES;
        let mut read = |probe: &str| {
            let mut renames = Vec::new();
            let mut next = Vec::new();
            for pointer in &active {
                let Some(range) = written_value_span(behavior, probe, pointer)
                    .and_then(|span| scalar_range(behavior, probe, span))
                else {
                    continue;
                };
                let done = wanted
                    .is_some_and(|token| range.start <= token.start && token.end <= range.end);
                found.entry(pointer.clone()).or_default().push(range);
                if done || last {
                    continue;
                }
                // Another copy of the key: renamed in the next stand-in.
                let Some(key) = key_range(behavior, probe, pointer) else {
                    continue;
                };
                let starts = starts.get_or_insert_with(|| line_starts(text));
                let (start, end) = (
                    byte_at(starts, probe, key.start),
                    byte_at(starts, probe, key.end),
                );
                let Some((at, character)) = probe
                    .get(start..end)
                    .and_then(|key| key.char_indices().find(|(_, c)| c.is_ascii_alphanumeric()))
                else {
                    continue;
                };
                renames.push((start + at, if character == 'a' { b'b' } else { b'a' }));
                next.push(pointer.clone());
            }
            (renames, next)
        };
        // The first reading is the caller's when it asks for one field.
        let current = probe.as_deref().unwrap_or(text);
        let (renames, next) = if round == 0 && active.len() == 1 {
            read(current)
        } else {
            with_spans(behavior, current, &active, || read(current))
        };
        if renames.is_empty() {
            break;
        }
        // ASCII for ASCII: every byte stays where it is.
        let mut bytes = probe
            .take()
            .unwrap_or_else(|| text.to_string())
            .into_bytes();
        for (at, renamed) in renames {
            bytes[at] = renamed;
        }
        let Ok(renamed) = String::from_utf8(bytes) else {
            break;
        };
        probe = Some(renamed);
        active = next;
    }
    found
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
