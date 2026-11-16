//! A long value a problem message quotes, cut short: never inside a
//! placeholder (half a token on the page is a piece of one) nor inside a
//! character as the user sees it.

use std::collections::{BTreeSet, HashSet};
use std::ops::Range;

use unicode_segmentation::GraphemeCursor;

use super::PlaceholderPattern;

/// Longest value quoted whole in a message; a longer one is cut with `…`.
const MAX_QUOTED_VALUE: usize = 80;

/// Texts a cut never ends inside (half a token on the page is a piece of
/// one, which P1 forbids): the host's placeholder tokens, looked up by
/// their lengths rather than searched for one by one, and what a
/// placeholder pattern finds (another document's token pasted here).
#[derive(Default)]
pub(in crate::editor) struct Tokens<'p> {
    texts: HashSet<String>,
    lengths: BTreeSet<usize>,
    patterns: &'p [PlaceholderPattern],
}

impl<'p> Tokens<'p> {
    pub(super) fn new<'t>(
        texts: impl IntoIterator<Item = &'t str>,
        patterns: &'p [PlaceholderPattern],
    ) -> Self {
        let texts: HashSet<String> = texts
            .into_iter()
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .collect();
        let lengths = texts.iter().map(String::len).collect();
        Self {
            texts,
            lengths,
            patterns,
        }
    }

    /// The byte ranges of `text` the patterns find.
    fn found(&self, text: &str) -> Vec<Range<usize>> {
        self.patterns
            .iter()
            .flat_map(|pattern| pattern(text))
            .collect()
    }

    /// The start of the first token of `text` (`found` holding what the
    /// patterns find there) that byte `at` falls strictly inside.
    fn around(&self, text: &str, at: usize, found: &[Range<usize>]) -> Option<usize> {
        let known = self.lengths.iter().filter_map(|&length| {
            (at.saturating_sub(length - 1)..at).find(|&start| {
                text.get(start..start + length)
                    .is_some_and(|piece| self.texts.contains(piece))
            })
        });
        let matched = found
            .iter()
            .filter(|range| range.start < at && at < range.end)
            .map(|range| range.start);
        known.chain(matched).min()
    }
}

/// `text` (a value as JSON), cut with `…` past [`MAX_QUOTED_VALUE`]
/// characters, before a token or a character it would split.
pub(super) fn cut(text: &str, tokens: &Tokens<'_>) -> String {
    match cut_point(text, MAX_QUOTED_VALUE, tokens) {
        Some(end) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// Where `text` longer than `max` characters is cut: at most `max`
/// characters in, never inside one of `tokens` nor inside a character as
/// the user sees it. `None` when it is not longer.
pub(super) fn cut_point(text: &str, max: usize, tokens: &Tokens<'_>) -> Option<usize> {
    let (mut end, _) = text.char_indices().nth(max)?;
    let found = tokens.found(text);
    loop {
        let before = grapheme_floor(text, tokens.around(text, end, &found).unwrap_or(end));
        if before == end {
            return Some(end);
        }
        end = before;
    }
}

/// The last place at or before byte `at` (a character's start) of `text`
/// where a character as the user sees it starts.
fn grapheme_floor(text: &str, at: usize) -> usize {
    let mut cursor = GraphemeCursor::new(at, text.len(), true);
    match cursor.is_boundary(text, 0) {
        Ok(false) => cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0),
        _ => at,
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_QUOTED_VALUE, Tokens, cut};

    /// A long value is cut before a placeholder it would split (the host's
    /// own, or one a placeholder pattern finds), and never inside a
    /// character drawn as one (P1: no piece of a token shows).
    #[test]
    fn a_cut_value_keeps_its_tokens_and_characters_whole() {
        let token = "«secret 3f2a»";
        let other = "«secret 9b1c»";
        let pattern: super::PlaceholderPattern = Box::new(move |text: &str| {
            text.match_indices(other)
                .map(|(at, found)| at..at + found.len())
                .collect()
        });
        let patterns = [pattern];
        let tokens = Tokens::new([token], &patterns);
        let pieces = [
            token,
            other,
            "👨\u{200d}👩\u{200d}👧",
            "e\u{301}",
            "🇫🇷",
            "日",
            "\u{1112}\u{1161}\u{11ab}",
            "क्षि",
            "\u{0995}\u{09cd}\u{09b7}",
            "\u{0e01}\u{0e33}",
        ];
        for piece in pieces {
            let width = piece.chars().count();
            for lead in MAX_QUOTED_VALUE.saturating_sub(width + 1)..=MAX_QUOTED_VALUE {
                let head = "a".repeat(lead);
                let text = format!("{head}{piece}{}", "b".repeat(20));
                let shown = cut(&text, &tokens);
                let kept = shown.strip_suffix('…').expect("a long value is cut");
                assert!(text.starts_with(kept), "{shown:?}");
                assert!(kept.chars().count() <= MAX_QUOTED_VALUE, "{shown:?}");
                assert!(
                    kept.len() <= head.len() || kept.len() >= head.len() + piece.len(),
                    "{piece:?} split in {shown:?}"
                );
            }
        }
        // The token is kept when it fits, and a short value is not cut.
        let text = format!("{token}{}", "b".repeat(100));
        assert!(cut(&text, &tokens).starts_with(token));
        assert_eq!(cut(token, &tokens), token);
    }

    /// A problem quoting a long value holding one of the host's
    /// placeholders, in a field it does not stand for, never shows part of
    /// it (P1).
    #[cfg(feature = "yaml")]
    #[test]
    fn a_problem_never_quotes_part_of_a_placeholder() {
        let token = "«secret 3f2a»";
        let schema =
            crate::editor::schema::Schema::compile(r#"{"properties": {"a": {"type": "string"}}}"#)
                .expect("compiles");
        let accepted = [(token.to_string(), "/b".to_string())];
        // For some of these leads the token starts before the cut and ends
        // after it.
        for lead in 57..80 {
            let text = format!("a:\n  note: {}{token}\nb: {token}\n", "x".repeat(lead));
            let problems = super::super::validate_with(
                &text,
                Some(&schema.bundle),
                None,
                true,
                crate::editor::Language::YAML.behavior(),
                &accepted,
                crate::strings::default_strings(),
            );
            assert!(
                problems.iter().any(|problem| problem.message.contains('…')),
                "{problems:#?}"
            );
            for problem in &problems {
                let marks = problem.message.matches('«').count();
                assert_eq!(
                    marks,
                    problem.message.matches(token).count(),
                    "{problem:#?}"
                );
            }
        }
    }
}
