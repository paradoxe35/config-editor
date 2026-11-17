//! `around_written` looks the text at each place up among the tokens'
//! texts: a keystroke costs the same with thousands of placeholders as
//! with one, and the region is widened as comparing each token's text
//! everywhere does.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use std::ops::Range;

use super::super::text::{TextRange, TextSize, byte_range, text_size};
use super::super::work_count::{Work, counted};
use super::rewritten::around_written;

/// Each token's text compared at every place (the reference), in bytes:
/// a token's text starts on a character, so its bytes are found only at
/// whole characters.
fn compared_everywhere(
    texts: &[&str],
    before: &str,
    after: &str,
    region: &Range<usize>,
) -> Range<usize> {
    let texts: Vec<&[u8]> = texts
        .iter()
        .filter(|text| !text.is_empty())
        .map(|text| text.as_bytes())
        .collect();
    let Some(longest) = texts.iter().map(|text| text.len()).max() else {
        return region.clone();
    };
    let new = after.as_bytes();
    let old_len = before.len();
    let Some(inserted) = (new.len() + region.len()).checked_sub(old_len) else {
        return region.clone();
    };
    let across = |edge: usize, start: bool| {
        let from = edge.saturating_sub(longest);
        let to = (edge + longest).min(new.len());
        let window = &new[from..to];
        texts
            .iter()
            .filter_map(|text| {
                window
                    .windows(text.len())
                    .enumerate()
                    .map(|(at, piece)| (from + at, piece))
                    .find(|(at, piece)| *at < edge && at + text.len() > edge && piece == text)
                    .map(|(at, _)| {
                        if start {
                            edge - at
                        } else {
                            at + text.len() - edge
                        }
                    })
            })
            .max()
            .unwrap_or(0)
    };
    let (mut start, mut end) = (region.start, region.end);
    let mut written_end = region.start + inserted;
    loop {
        let before_by = across(start, true);
        let after_by = across(written_end, false).min(old_len - end);
        if before_by == 0 && after_by == 0 {
            return start..end;
        }
        start -= before_by;
        end += after_by;
        written_end += after_by;
    }
}

struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from((self.0 >> 33) % bound as u64).expect("below a usize bound")
    }

    fn text(&mut self, max: usize) -> String {
        const CHARS: [char; 6] = ['a', 'b', '«', '»', '\n', '😀'];
        (0..self.below(max + 1))
            .map(|_| CHARS[self.below(CHARS.len())])
            .collect()
    }
}

#[test]
fn widens_as_comparing_every_token_everywhere_does() {
    let mut random = Random(0xa20);
    for _ in 0..20_000 {
        let tokens: Vec<String> = (0..1 + random.below(4)).map(|_| random.text(4)).collect();
        let texts: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let head = random.text(10);
        let tail = random.text(10);
        let removed = random.text(3);
        let inserted = random.text(3);
        let before = format!("{head}{removed}{tail}");
        let after = format!("{head}{inserted}{tail}");
        let region = head.len()..head.len() + removed.len();
        let widened = around_written(&texts, &before, &after, &byte_range(region.clone()));
        assert_eq!(
            Range::<usize>::from(widened),
            compared_everywhere(&texts, &before, &after, &region),
            "{texts:?}: {before:?} → {after:?} over {region:?}"
        );
    }
}

#[test]
fn a_keystroke_looks_up_the_text_around_it_once_however_many_placeholders() {
    let tokens: Vec<String> = (0..3_000)
        .map(|index| format!("«secret {index:08x}»"))
        .collect();
    let texts: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let before: String = tokens
        .iter()
        .map(|token| format!("  key: {token}\n"))
        .collect();
    let at = before.len() / 2;
    let at = at + before[at..].find('\n').unwrap();
    let mut after = before.clone();
    after.insert(at, 'x');
    let start = TextRange::empty(text_size(at));
    let (widened, lookups) = counted(Work::TokenScan, || {
        around_written(&texts, &before, &after, &start)
    });
    assert_eq!(widened, start);
    // A place on each side of the edit within the longest token's reach
    // (its bytes), one lookup each: not a pass over the 3,000 tokens.
    let longest = usize::from(TextSize::of(texts[0]));
    assert!(lookups <= 2 * longest, "{lookups} lookups");
}
