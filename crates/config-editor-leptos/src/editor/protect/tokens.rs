//! Secret placeholders: tokens such as `«secret 3f2a»` the host writes in
//! place of real secrets. A token is real only in the field the host gave
//! it in (the path whose value held it), at most as often as there: moved
//! into another field (pasted, or its key renamed) it is plain text and
//! flagged, so it can never stand for another field's secret; put back in
//! its own field it is real again, since the value is then what the host
//! gave. Each real token is atomic (the caret never lands inside one, and
//! an edit touching one replaces it whole).

use std::ops::Range;

use leptos::prelude::Callable;
use leptos::prelude::Callback;

use super::super::language::ConfigLanguage;
use super::super::paths::{path_at, same_field};
use super::marks::escape;

/// A token of the host's text: the field it stands in, and how often that
/// field's value holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Known {
    pub(crate) token: String,
    /// JSON pointer of the field (see [`same_field`]).
    pub(crate) pointer: String,
    pub(crate) count: usize,
}

/// The pointer of the field holding the token at byte `start` of `text`.
fn field_at(behavior: &dyn ConfigLanguage, text: &str, start: usize) -> String {
    path_at(behavior, text, text[..start].encode_utf16().count())
        .map(|path| path.to_pointer())
        .unwrap_or_default()
}

use super::super::secret_options::SecretOptions;

/// [`SecretOptions`] made ready to match.
pub(crate) struct SecretMatcher {
    pattern: Option<regex_lite::Regex>,
    is_placeholder: Option<Callback<String, bool>>,
}

impl SecretMatcher {
    /// `None` pattern when it does not compile: no token is ever real then.
    pub(crate) fn new(options: &SecretOptions) -> Self {
        SecretMatcher {
            pattern: regex_lite::Regex::new(&options.placeholder).ok(),
            is_placeholder: options.is_placeholder,
        }
    }

    /// Byte ranges of the tokens the pattern (and the host check) accept in
    /// `text`.
    pub(crate) fn find(&self, text: &str) -> Vec<Range<usize>> {
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
    /// tokens); a text that does not parse is read token by token instead.
    pub(crate) fn known(&self, behavior: &dyn ConfigLanguage, text: &str) -> Vec<Known> {
        let mut known: Vec<Known> = Vec::new();
        let mut add = |token: &str, pointer: String| match known
            .iter_mut()
            .find(|seen| seen.token == token && same_field(&seen.pointer, &pointer))
        {
            Some(seen) => seen.count += 1,
            None => known.push(Known {
                token: token.to_string(),
                pointer,
                count: 1,
            }),
        };
        match behavior.parse(text) {
            Ok(document) => {
                let mut stack = vec![(String::new(), &document)];
                while let Some((pointer, value)) = stack.pop() {
                    match value {
                        serde_json::Value::String(written) => {
                            for range in self.find(written) {
                                add(&written[range], pointer.clone());
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
            Err(_) => {
                for range in self.find(text) {
                    let pointer = field_at(behavior, text, range.start);
                    add(&text[range], pointer);
                }
            }
        }
        known
    }
}

/// UTF-16 ranges of the known tokens written in `text`, each as often as
/// the host wrote it, first ones first, without asking where each is (a
/// text replaced whole: an undo, a switch); the pause in typing then checks
/// they are in their fields (`EditorState::recheck_tokens`).
pub(crate) fn locate_written(text: &str, known: &[Known]) -> Vec<Range<usize>> {
    let mut tokens: Vec<(&str, usize)> = Vec::new();
    for known in known {
        match tokens.iter_mut().find(|(token, _)| *token == known.token) {
            Some((_, count)) => *count += known.count,
            None => tokens.push((&known.token, known.count)),
        }
    }
    let mut found: Vec<Range<usize>> = Vec::new();
    for (token, count) in tokens {
        found.extend(
            text.match_indices(token)
                .take(count)
                .map(|(byte, written)| {
                    let start = text[..byte].encode_utf16().count();
                    start..start + written.encode_utf16().count()
                }),
        );
    }
    found.sort_by_key(|range| range.start);
    found.dedup_by(|later, earlier| later.start < earlier.end);
    found
}

/// How many times the parsed `document` holds `token` in the fields the
/// host gave it in (`known`), at most as often as there.
pub(crate) fn held_in_fields(document: &serde_json::Value, known: &[Known], token: &str) -> usize {
    known
        .iter()
        .filter(|known| known.token == token)
        .map(|known| {
            let value = document.pointer(&known.pointer).or_else(|| {
                let lone = format!("{}/", known.pointer).replace("/0/", "/");
                document.pointer(lone.trim_end_matches('/'))
            });
            value
                .and_then(serde_json::Value::as_str)
                .map_or(0, |value| value.matches(token).count().min(known.count))
        })
        .sum()
}

/// UTF-16 ranges of the real tokens in `text` (written in `behavior`):
/// each `known` token in its own field, at most as often as the host wrote
/// it there, first ones first.
pub(crate) fn locate(
    behavior: &dyn ConfigLanguage,
    text: &str,
    known: &[Known],
) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    let mut tokens: Vec<&str> = known.iter().map(|known| known.token.as_str()).collect();
    tokens.sort_unstable();
    tokens.dedup();
    for token in tokens {
        let mut left: Vec<(&str, usize)> = known
            .iter()
            .filter(|known| known.token == token)
            .map(|known| (known.pointer.as_str(), known.count))
            .collect();
        for (byte, written) in text.match_indices(token) {
            let pointer = field_at(behavior, text, byte);
            let Some((_, count)) = left
                .iter_mut()
                .find(|(field, count)| *count > 0 && same_field(field, &pointer))
            else {
                continue;
            };
            *count -= 1;
            let start = text[..byte].encode_utf16().count();
            found.push(start..start + written.encode_utf16().count());
        }
    }
    found.sort_by_key(|range| range.start);
    found.dedup_by(|later, earlier| later.start < earlier.end);
    found
}

/// The edit turning `before` into `after` (whose replaced region is
/// `region`, UTF-16 in `before`), widened so it never cuts a token: a
/// token it touches goes whole. `None` when it touches none.
pub(crate) fn widen(
    tokens: &[Range<usize>],
    before: &str,
    after: &str,
    region: Range<usize>,
) -> Option<(String, Range<usize>, usize)> {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let inserted_len = (new.len() + region.len()).checked_sub(old.len())?;
    let inserted = &new[region.start..region.start + inserted_len];
    let mut wide = region.clone();
    loop {
        let grown = tokens.iter().fold(wide.clone(), |wide, token| {
            if cuts(token, &wide) {
                wide.start.min(token.start)..wide.end.max(token.end)
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
    let mut units = old[..wide.start].to_vec();
    units.extend_from_slice(inserted);
    units.extend_from_slice(&old[wide.end..]);
    Some((
        String::from_utf16_lossy(&units),
        wide.clone(),
        wide.start + inserted_len,
    ))
}

/// Whether an edit replacing `region` cuts `token`: it removes part of it,
/// or inserts inside it.
fn cuts(token: &Range<usize>, region: &Range<usize>) -> bool {
    if region.is_empty() {
        return token.start < region.start && region.start < token.end;
    }
    let overlaps = region.start < token.end && token.start < region.end;
    let covers = region.start <= token.start && token.end <= region.end;
    overlaps && !covers
}

/// The tokens after an edit replaced `region` (UTF-16 in the old text)
/// with `inserted_len` units: those before stay, those after shift, those
/// it touched are gone.
pub(crate) fn follow(
    tokens: &[Range<usize>],
    region: &Range<usize>,
    inserted_len: usize,
) -> Vec<Range<usize>> {
    tokens
        .iter()
        .filter_map(|token| {
            if token.end <= region.start {
                Some(token.clone())
            } else if token.start >= region.end {
                let shift = |at: usize| at + inserted_len - region.len();
                Some(shift(token.start)..shift(token.end))
            } else {
                None
            }
        })
        .collect()
}

/// UTF-16 `offset` moved out of any token: to the edge it was moving
/// towards (`forward`), or the nearest one.
pub(crate) fn snap(tokens: &[Range<usize>], offset: usize, forward: Option<bool>) -> usize {
    let Some(token) = tokens
        .iter()
        .find(|token| token.start < offset && offset < token.end)
    else {
        return offset;
    };
    match forward {
        Some(true) => token.end,
        Some(false) => token.start,
        None if offset - token.start <= token.end - offset => token.start,
        None => token.end,
    }
}

/// A selection grown to take whole every token it cuts.
pub(crate) fn snap_selection(tokens: &[Range<usize>], start: usize, end: usize) -> (usize, usize) {
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
mod tests {
    use super::*;

    const TOKEN: &str = "«secret 3f2a»";

    #[cfg(all(feature = "yaml", feature = "json"))]
    #[test]
    fn only_host_tokens_are_real_in_their_own_field_and_at_most_as_often() {
        use super::super::super::Language;
        let matcher = SecretMatcher::new(&SecretOptions::default());
        let host = format!("a: {TOKEN}\nb: «secret zz»\n");
        let yaml = Language::YAML.behavior();
        let known = matcher.known(yaml, &host);
        assert_eq!(
            known,
            vec![Known {
                token: TOKEN.to_string(),
                pointer: "/a".to_string(),
                count: 1
            }]
        );
        // Shown as JSON: real in `a`, plain in `c`.
        let shown = format!("{{\"c\": \"{TOKEN}\", \"a\": \"{TOKEN}\"}}");
        let start = shown
            .rfind(TOKEN)
            .map(|at| shown[..at].encode_utf16().count())
            .unwrap();
        let json = Language::JSON.behavior();
        assert_eq!(locate(json, &shown, &known), vec![start..start + 13]);
        let custom = SecretMatcher::new(
            &SecretOptions::default().with_matcher(|token| token.ends_with("2a»")),
        );
        assert_eq!(
            custom.known(yaml, "x: «secret 11»\ny: «secret 2a»").len(),
            1
        );
        assert!(same_field("/a/0/b", "/a/b") && !same_field("/a", "/b"));
    }

    #[test]
    fn edits_touching_a_token_take_it_whole() {
        let text = format!("k: {TOKEN} x");
        let token = 3..16;
        let tokens = [token.clone()];
        // Backspace after it, Delete before it.
        let backspace = format!("k: {} x", &TOKEN[..TOKEN.len() - '»'.len_utf8()]);
        assert_eq!(
            widen(&tokens, &text, &backspace, 15..16),
            Some(("k:  x".to_string(), token.clone(), 3))
        );
        let delete = format!("k: {} x", &TOKEN['«'.len_utf8()..]);
        assert_eq!(
            widen(&tokens, &text, &delete, 3..4).map(|(text, ..)| text),
            Some("k:  x".to_string())
        );
        // Typing over a selection that ends inside it.
        let units: Vec<u16> = text.encode_utf16().collect();
        let typed = format!("k:Z{}", String::from_utf16_lossy(&units[6..]));
        assert_eq!(
            widen(&tokens, &text, &typed, 2..6).map(|(text, ..)| text),
            Some("k:Z x".to_string())
        );
        // Typing at its edges, or deleting it whole, cuts nothing.
        assert_eq!(widen(&tokens, &text, &format!("k: Q{TOKEN} x"), 3..3), None);
        assert_eq!(
            widen(&tokens, &text, &format!("k: {TOKEN}Q x"), 16..16),
            None
        );
        assert_eq!(widen(&tokens, &text, "k:  x", 3..16), None);
    }

    #[test]
    fn the_caret_snaps_out_of_tokens() {
        let tokens = vec![3..16, 20..33];
        assert_eq!(snap(&tokens, 5, None), 3);
        assert_eq!(snap(&tokens, 14, None), 16);
        assert_eq!(snap(&tokens, 5, Some(true)), 16);
        assert_eq!(snap(&tokens, 3, Some(true)), 3);
        assert_eq!(snap_selection(&tokens, 5, 25), (3, 33));
    }
}
