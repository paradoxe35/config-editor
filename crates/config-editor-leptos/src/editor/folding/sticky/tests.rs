//! Which lines stick: the chain of blocks holding the top of the view, in
//! every language (a custom one too), with folds closed, in an Edit-as
//! view; the push-up at a block's end; the caret's room below them; and a
//! seeded property against a reference written the slow way.

use super::super::super::fold_ranges::{FoldRange, default_fold_ranges};
use super::super::super::languages::Language;
use super::super::super::projection::Projection;
use super::super::super::test_state::TestEditor;
use super::super::super::work_count::{Work, counted};
use super::scopes::{Scopes, View};
use crate::EditorSettings;
use crate::language::{ConfigLanguage, LanguageConfig, LanguageInfo, ParseError};
use leptos::prelude::*;

/// A language of the editor's users: `;` comments, `( )` blocks, and the
/// default fold ranges (indentation and brackets).
struct Parens;

impl ConfigLanguage for Parens {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "Parens",
            icon: "P",
            extensions: &[],
        }
    }
    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &[";"],
            brackets: &[('(', ')')],
            ..LanguageConfig::default()
        }
    }
    fn parse(&self, _source: &str) -> Result<serde_json::Value, ParseError> {
        Ok(serde_json::Value::Null)
    }
}

static PARENS: Parens = Parens;

fn scopes_of(language: Language, text: &str, folds: &[FoldRange]) -> Scopes {
    let ranges = language.behavior().fold_ranges(text);
    Scopes::new(&ranges, &Projection::new(text, folds))
}

/// The sticky lines with `marker`'s line at the view's top, as their text.
fn headers_at(language: Language, text: &str, marker: &str, max_lines: usize) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let row = lines
        .iter()
        .position(|line| line.trim_start().starts_with(marker))
        .expect(marker);
    scopes_of(language, text, &[])
        .lines_at(row, max_lines)
        .into_iter()
        .map(|line| lines[line].trim().to_string())
        .collect()
}

/// Every language's spelling of the same nesting: `a` holds `b` holds `c`,
/// whose first entry is `d`, each block with entries after it.
fn nested_cases() -> Vec<(Language, &'static str, &'static str, Vec<&'static str>)> {
    let mut cases: Vec<(Language, &'static str, &'static str, Vec<&'static str>)> = vec![(
        Language::new(&PARENS),
        "(a\n  (b\n    (c\n      d 1\n      e 2\n      f 3\n      g 4)\n    h 5)\n  i 6)\nj 7",
        "d",
        vec!["(a", "(b", "(c"],
    )];
    #[cfg(feature = "yaml")]
    cases.push((
        Language::YAML,
        "a:\n  b:\n    c:\n      d: 1\n      e: 2\n      f: 3\n      g: 4\n    h: 5\n  i: 6\nj: 7",
        "d:",
        vec!["a:", "b:", "c:"],
    ));
    #[cfg(feature = "json")]
    cases.push((
        Language::JSON,
        "{\n  \"a\": {\n    \"b\": {\n      \"c\": {\n        \"d\": 1,\n        \"e\": 2,\n        \"f\": 3,\n        \"g\": 4\n      },\n      \"h\": 5\n    },\n    \"i\": 6\n  },\n  \"j\": 7\n}",
        "\"d\"",
        vec!["{", "\"a\": {", "\"b\": {", "\"c\": {"],
    ));
    #[cfg(feature = "toml")]
    cases.push((
        Language::TOML,
        "title = 1\n\n[a.b.c]\nd = 1\ne = 2\nf = 3\ng = 4\n\n[a.b]\nh = 5",
        "d =",
        vec!["[a.b.c]"],
    ));
    #[cfg(feature = "hcl")]
    cases.push((
        Language::HCL,
        "a {\n  b {\n    c {\n      d = 1\n      e = 2\n      f = 3\n      g = 4\n    }\n    h = 5\n  }\n  i = 6\n}\nj = 7",
        "d =",
        vec!["a {", "b {", "c {"],
    ));
    #[cfg(feature = "xml")]
    cases.push((
        Language::XML,
        "<a>\n  <b>\n    <c>\n      <d>1</d>\n      <e>2</e>\n      <f>3</f>\n      <g>4</g>\n    </c>\n    <h>5</h>\n  </b>\n  <i>6</i>\n</a>",
        "<d>",
        vec!["<a>", "<b>", "<c>"],
    ));
    cases
}

#[test]
fn the_headers_of_the_blocks_above_stick_outermost_first_in_every_language() {
    let failures: Vec<String> = nested_cases()
        .into_iter()
        .filter_map(|(language, text, marker, wanted)| {
            let got = headers_at(language, text, marker, 5);
            (got != wanted).then(|| format!("{}: {got:?}, wanted {wanted:?}", language.label()))
        })
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn at_most_the_lines_asked_for_stick_the_outermost_kept() {
    for (language, text, marker, wanted) in nested_cases() {
        let got = headers_at(language, text, marker, 1);
        assert_eq!(got, wanted[..1], "{}", language.label());
    }
}

#[test]
fn every_language_sticks_what_the_reference_finds_at_every_row() {
    let failures: Vec<String> = nested_cases()
        .into_iter()
        .flat_map(|(language, text, _, _)| {
            let ranges = language.behavior().fold_ranges(text);
            let scopes = Scopes::new(&ranges, &Projection::new(text, &[]));
            let rows = text.split('\n').count();
            (0..rows)
                .flat_map(move |row| (1..=6).map(move |max| (row, max)))
                .filter_map(move |(row, max)| {
                    let got = scopes.lines_at(row, max);
                    let wanted = reference(&ranges, row, max);
                    (got != wanted).then(|| {
                        format!(
                            "{} row {row} max {max}: {got:?}, wanted {wanted:?}",
                            language.label()
                        )
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_header_still_in_view_does_not_stick() {
    // The top on `x`'s line: `a` sticks over it, and `b` is the line
    // right below, in view.
    #[cfg(feature = "yaml")]
    assert_eq!(
        scopes_of(Language::YAML, "a:\n  x: 1\n  b:\n    c: 1\n    d: 2", &[]).lines_at(1, 5),
        [0]
    );
    let text = "(a\n  x\n  (b\n    c\n    d))";
    let scopes = scopes_of(Language::new(&PARENS), text, &[]);
    assert_eq!(scopes.lines_at(0, 5), Vec::<usize>::new());
    assert_eq!(scopes.lines_at(1, 5), [0]);
    assert_eq!(scopes.lines_at(2, 5), [0, 2]);
}

#[test]
fn the_innermost_line_is_pushed_up_as_its_block_ends() {
    let text =
        "(a\n  (b\n    (c\n      d 1\n      e 2\n      f 3\n      g 4)\n    h 5)\n  i 6)\nj 7";
    let scopes = scopes_of(Language::new(&PARENS), text, &[]);
    let at = |top: f64| {
        let layout = scopes.layout(View {
            top,
            line_height: 10.0,
            max_lines: 5,
        });
        let tops: Vec<f64> = layout.lines.iter().map(|line| line.top).collect();
        (tops, layout.height)
    };
    // `(c` ends on row 6: at row 4.1 its line is at depth 2, a pixel up.
    assert_eq!(at(40.0), (vec![0.0, 10.0, 20.0], 30.0));
    assert_eq!(at(41.0), (vec![0.0, 10.0, 19.0], 29.0));
    assert_eq!(at(45.0), (vec![0.0, 10.0, 15.0], 25.0));
    // Its last row under the second line: `(c` is gone, `(b` stays.
    assert_eq!(at(50.0), (vec![0.0, 10.0], 20.0));
    // `(b` ends on row 7 and `(a` on row 8: each pushed in its turn.
    assert_eq!(at(65.0), (vec![0.0, 5.0], 15.0));
    assert_eq!(at(75.0), (vec![0.0], 10.0));
    assert_eq!(at(85.0), (vec![-5.0], 5.0));
    assert_eq!(at(90.0), (vec![], 0.0));
}

#[test]
fn a_fold_closed_counts_as_one_row_and_its_block_holds_nothing() {
    let text = "(a\n  (b\n    (c\n      d 1\n      e 2)\n    f 3)\n  g 4)\nh 5";
    let language = Language::new(&PARENS);
    // `(c` closed: rows are `(a`, `(b`, `(c…`, `f 3)`, `g 4)`, `h 5`.
    let scopes = scopes_of(language, text, &[FoldRange::new(2, 4)]);
    assert_eq!(scopes.lines_at(2, 5), [0, 1]);
    assert_eq!(scopes.lines_at(3, 5), [0]);
    // `(b` closed: the row after it is `g 4)`, line 6.
    let scopes = scopes_of(language, text, &[FoldRange::new(1, 5)]);
    assert_eq!(scopes.lines_at(1, 5), [0]);
    assert_eq!(scopes.lines_at(2, 5), [0]);
    assert_eq!(scopes.lines_at(3, 5), Vec::<usize>::new());
}

#[test]
fn the_caret_row_is_revealed_just_below_the_lines_that_stay() {
    let text =
        "(a\n  (b\n    (c\n      d 1\n      e 2\n      f 3\n      g 4)\n    h 5)\n  i 6)\nj 7";
    let scopes = scopes_of(Language::new(&PARENS), text, &[]);
    let view = |top: f64| View {
        top,
        line_height: 10.0,
        max_lines: 5,
    };
    // Row 4 (`e 2`) goes under `(a`, `(b` and `(c`.
    assert_eq!(scopes.reveal_top(4, 10.0, 5), 10.0);
    assert!(!scopes.covers(view(10.0), 4));
    assert!(scopes.covers(view(40.0), 5));
    // A header row goes below its own parents' lines.
    assert_eq!(scopes.reveal_top(2, 10.0, 5), 0.0);
    // At the top of the text nothing sticks.
    assert_eq!(scopes.reveal_top(0, 10.0, 5), 0.0);
}

#[cfg(all(feature = "yaml", feature = "json"))]
#[test]
fn an_edit_as_view_sticks_its_own_languages_headers() {
    let host =
        "a:\n  b:\n    c:\n      d: 1\n      e: 2\n      f: 3\n      g: 4\n    h: 5\n  i: 6\nj: 7";
    let mut editor = TestEditor::new(Language::YAML, host);
    editor.edit_as = Some(Language::JSON);
    let (_owner, state) = editor.mount();
    state.settle_readings();
    let text = state.doc().text.get_untracked();
    let scopes = Scopes::new(
        &state.fold().blocks.get_untracked(),
        &state.view().projection.get_untracked(),
    );
    let lines: Vec<&str> = text.split('\n').collect();
    let row = lines
        .iter()
        .position(|line| line.contains("\"d\""))
        .unwrap_or_default();
    let headers: Vec<&str> = scopes
        .lines_at(row, 5)
        .into_iter()
        .map(|line| lines[line].trim())
        .collect();
    assert_eq!(headers, ["{", "\"a\": {", "\"b\": {", "\"c\": {"], "{text}");
}

#[cfg(feature = "yaml")]
#[test]
fn the_blocks_are_read_while_folding_or_sticky_scroll_is_on() {
    let text = "a:\n  b: 1\nc: 2";
    let cases = [
        (true, true, true, true),
        (false, true, true, false),
        (true, false, true, true),
        (false, false, false, false),
    ];
    for (folding, sticky_scroll, blocks, foldable) in cases {
        let mut editor = TestEditor::new(Language::YAML, text);
        editor.settings = Signal::stored(EditorSettings {
            folding,
            sticky_scroll,
            ..EditorSettings::default()
        });
        let (_owner, state) = editor.mount();
        let fold = state.fold();
        assert_eq!(
            (
                !fold.blocks.get_untracked().is_empty(),
                !fold.fold_ranges.get_untracked().is_empty()
            ),
            (blocks, foldable),
            "folding {folding}, sticky scroll {sticky_scroll}"
        );
    }
}

/// The custom language's fold ranges are the default ones.
#[test]
fn a_custom_language_gets_the_default_blocks() {
    let text = "(a\n  b\n  c)";
    assert_eq!(
        Language::new(&PARENS).behavior().fold_ranges(text),
        default_fold_ranges(text, &PARENS.config(), &[])
    );
}

/// P5: finding the sticky lines at a scroll position is a binary search and
/// a walk up the blocks per line, the same in a document twice as long.
#[test]
fn a_scroll_finds_its_sticky_lines_without_a_pass_over_the_document() {
    let steps = |blocks: usize| {
        let mut text = String::new();
        for block in 0..blocks {
            text.push_str(&format!(
                "(a{block}\n  (b\n    (c\n      d 1\n      e 2)))\n"
            ));
        }
        let scopes = scopes_of(Language::new(&PARENS), &text, &[]);
        let middle = blocks / 2 * 5 + 3;
        counted(Work::StickyStep, || scopes.lines_at(middle, 5)).1
    };
    let (small, large) = (steps(200), steps(400));
    assert_eq!(small, large, "steps at 200 and 400 blocks");
    assert!(small <= 5 * 8, "{small} steps");
}

/// What sticks with the view's top on `row` exactly, written the slow way
/// from the definition over nested `ranges` (lines, nothing folded): at
/// each depth, the outermost range inside the last one that holds the row
/// at that depth, while its header is above that row.
fn reference(ranges: &[FoldRange], row: usize, max_lines: usize) -> Vec<usize> {
    let mut chain: Vec<FoldRange> = Vec::new();
    for depth in 0..max_lines {
        let at = row + depth;
        let inside = |range: &&FoldRange| {
            chain
                .last()
                .is_none_or(|last| last.start < range.start && range.end <= last.end)
        };
        let Some(outermost) = ranges
            .iter()
            .filter(|range| range.start <= at && at <= range.end)
            .filter(inside)
            .min_by_key(|range| (range.start, std::cmp::Reverse(range.end)))
        else {
            break;
        };
        if outermost.start >= at {
            break;
        }
        chain.push(*outermost);
    }
    chain.iter().map(|range| range.start).collect()
}

fn seed() -> u64 {
    std::env::var("STICKY_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(0x5f37_59df_9e37_79b9)
}

/// Random cases: `STICKY_CASES`, else `default` (the sweep raises it).
fn cases(default: usize) -> usize {
    std::env::var("STICKY_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).unwrap_or(0) % bound.max(1)
    }
}

/// Nested ranges over `lines` lines, sorted by start, one per start.
fn nested(rng: &mut Rng, lines: usize) -> Vec<FoldRange> {
    fn fill(rng: &mut Rng, start: usize, end: usize, out: &mut Vec<FoldRange>) {
        let mut line = start;
        while line < end {
            if rng.below(3) == 0 {
                line += 1;
                continue;
            }
            let last = line + 1 + rng.below(end - line);
            out.push(FoldRange::new(line, last));
            fill(rng, line + 1, last, out);
            line = last + 1;
        }
    }
    let mut out = Vec::new();
    fill(rng, 0, lines - 1, &mut out);
    out.sort_unstable();
    out
}

/// The first mismatch of the sticky lines against [`reference`] over
/// `ranges`, at every row and line count.
fn mismatch(ranges: &[FoldRange], lines: usize) -> Option<String> {
    let scopes = Scopes::new(ranges, &Projection::new(&"\n".repeat(lines - 1), &[]));
    (0..lines).find_map(|row| {
        (1..=4).find_map(|max| {
            let got = scopes.lines_at(row, max);
            let wanted = reference(ranges, row, max);
            (got != wanted).then(|| format!("row {row}, max {max}: {got:?}, wanted {wanted:?}"))
        })
    })
}

/// The sticky lines are the blocks holding the top, outermost first and
/// capped, on random nested blocks; and a revealed row is never under them.
/// A failure is shrunk to the fewest blocks that still fail.
#[test]
fn random_blocks_stick_as_the_reference_finds_and_never_cover_a_revealed_row() {
    let mut rng = Rng(seed());
    for case in 0..cases(300) {
        let lines = 2 + rng.below(40);
        let mut ranges = nested(&mut rng, lines);
        if mismatch(&ranges, lines).is_some() {
            let mut at = 0;
            while at < ranges.len() {
                let mut fewer = ranges.clone();
                fewer.remove(at);
                if mismatch(&fewer, lines).is_some() {
                    ranges = fewer;
                } else {
                    at += 1;
                }
            }
            panic!(
                "case {case} (seed {}): {ranges:?} over {lines} lines: {}",
                seed(),
                mismatch(&ranges, lines).unwrap_or_default()
            );
        }
        let scopes = Scopes::new(&ranges, &Projection::new(&"\n".repeat(lines - 1), &[]));
        let row = rng.below(lines);
        let max_lines = 1 + rng.below(5);
        let top = scopes.reveal_top(row, 10.0, max_lines);
        let view = View {
            top,
            line_height: 10.0,
            max_lines,
        };
        assert!(
            !scopes.covers(view, row) && row as f64 * 10.0 >= top,
            "case {case} (seed {}): row {row} under the sticky lines at {top} in {ranges:?}",
            seed()
        );
    }
}
