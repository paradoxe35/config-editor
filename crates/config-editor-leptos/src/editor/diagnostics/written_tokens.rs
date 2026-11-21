//! Where a text writes the host's secret placeholders, found in one pass
//! over the text for all of them (Aho-Corasick, the search `regex` makes
//! for many literals), not one pass for each: a document holds hundreds
//! of placeholders, and every check of its problems looks for them.

use std::ops::Range;

use aho_corasick::AhoCorasick;

use super::super::work_count::{Work, note};

/// Calls `found` with each place `text` writes one of `tokens` (the
/// token's place in `tokens`, and its bytes), each token's places as
/// `str::match_indices` finds them: left to right, apart.
fn each_place(text: &str, tokens: &[&str], mut found: impl FnMut(usize, Range<usize>)) {
    note(Work::TokenSearch);
    let searched: Vec<(usize, &str)> = tokens
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, token)| !token.is_empty())
        .collect();
    if searched.is_empty() {
        return;
    }
    let Ok(searcher) = AhoCorasick::new(searched.iter().map(|(_, token)| token)) else {
        // Too many to search at once (past the automaton's size limit):
        // a pass for each.
        for (at, token) in searched {
            for (start, _) in text.match_indices(token) {
                found(at, start..start + token.len());
            }
        }
        return;
    };
    // Every place of every token, overlapping ones included; each token's
    // in order (a token's places all have its length), the ones inside
    // its place before left out, as `match_indices` leaves them.
    let mut ends = vec![0; searched.len()];
    for place in searcher.find_overlapping_iter(text) {
        let pattern = place.pattern().as_usize();
        if place.start() >= ends[pattern] {
            ends[pattern] = place.end();
            found(searched[pattern].0, place.range());
        }
    }
}

/// Which of `tokens` `text` writes (an empty one, as `str::contains`
/// finds it, everywhere).
pub(super) fn written<'t>(text: &str, tokens: &[&'t str]) -> Vec<&'t str> {
    let mut seen: Vec<bool> = tokens.iter().map(|token| token.is_empty()).collect();
    each_place(text, tokens, |at, _| seen[at] = true);
    tokens
        .iter()
        .zip(seen)
        .filter_map(|(token, seen)| seen.then_some(*token))
        .collect()
}

/// Where `text` writes each of `tokens` (an empty one nowhere).
pub(super) fn places(text: &str, tokens: &[&str]) -> Vec<Range<usize>> {
    let mut places = Vec::new();
    each_place(text, tokens, |_, place| places.push(place));
    places
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The places and tokens written are those of a search for each token
    /// alone, also where tokens overlap, repeat, run into themselves or
    /// stand inside one another.
    #[test]
    fn one_search_finds_what_a_search_for_each_finds() {
        let texts = [
            "",
            "a «secret 1» b «secret 12» «secret 1»",
            "aaaa aaa",
            "«s»«s»«s» ««s»»",
            "é«é»é «é»",
        ];
        let tokens: [&[&str]; 5] = [
            &[],
            &["«secret 1»", "«secret 12»", "«secret 2»"],
            &["aa", "a", "aaa", ""],
            &["«s»", "«s»«s»", "s", "«s»"],
            &["«é»", "é«", "x"],
        ];
        let mut failures = Vec::new();
        for text in texts {
            for tokens in tokens {
                let mut expected: Vec<Range<usize>> = tokens
                    .iter()
                    .filter(|token| !token.is_empty())
                    .flat_map(|token| {
                        text.match_indices(token)
                            .map(|(at, _)| at..at + token.len())
                    })
                    .collect();
                let mut found = places(text, tokens);
                expected.sort_by_key(|range| (range.start, range.end));
                found.sort_by_key(|range| (range.start, range.end));
                if found != expected {
                    failures.push(format!("{text:?} {tokens:?}: {found:?}, not {expected:?}"));
                }
                let contained: Vec<&str> = tokens
                    .iter()
                    .copied()
                    .filter(|token| text.contains(token))
                    .collect();
                if written(text, tokens) != contained {
                    failures.push(format!(
                        "{text:?} {tokens:?}: written {:?}",
                        written(text, tokens)
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
