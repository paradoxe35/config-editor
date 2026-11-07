//! Fold ranges: the building blocks languages combine in
//! [`ConfigLanguage::fold_ranges`](crate::language::ConfigLanguage::fold_ranges).
//! Each reads the text only; none knows a language.

use std::ops::Range;

use super::super::language::LanguageConfig;

/// Lines that fold together: `start` stays visible with a `…` after it, and
/// the lines after it through `end` (0-based, inclusive) are hidden.
///
/// A closing bracket or tag on a line of its own stays visible, as in VS
/// Code: `{` on line 3 closed by `}` alone on line 9 folds `3..=8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FoldRange {
    /// The line that stays visible (0-based).
    pub start: usize,
    /// The last hidden line (0-based), after `start`.
    pub end: usize,
}

impl FoldRange {
    /// The range `start..=end`.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Whether `line` is one this range hides.
    pub fn hides(&self, line: usize) -> bool {
        line > self.start && line <= self.end
    }

    /// Whether `line` is in the range, its first line included.
    pub fn contains(&self, line: usize) -> bool {
        line >= self.start && line <= self.end
    }
}

/// `ranges` sorted by start line, one per start (the longest), without
/// ranges that hide nothing or run past the text's `line_count`.
pub fn merge_fold_ranges(mut ranges: Vec<FoldRange>, line_count: usize) -> Vec<FoldRange> {
    ranges.retain(|range| range.end > range.start && range.end < line_count);
    ranges.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    ranges.dedup_by_key(|range| range.start);
    ranges
}

/// What a line is to [`indent_fold_ranges`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IndentLine {
    /// Structure: it opens a fold over the more indented lines after it.
    Code,
    /// Ignored, like a blank line (a comment).
    Skip,
    /// Inside a fold, never opening one (the body of a multi-line string).
    Text,
}

/// Folds by indentation, as VS Code does without a language's help: a line
/// folds the lines after it that are more indented, through the last such
/// line that is not blank or skipped.
///
/// `kind` tells what each line is (by 0-based index and text). `continues`
/// keeps a fold open over a line at the same indent as its first one
/// (`(first, line)`): a sequence written under its key at the key's indent.
pub fn indent_fold_ranges(
    source: &str,
    kind: impl Fn(usize, &str) -> IndentLine,
    continues: impl Fn(&str, &str) -> bool,
) -> Vec<FoldRange> {
    struct Open<'a> {
        indent: usize,
        line: usize,
        text: &'a str,
        last: usize,
    }
    let mut ranges = Vec::new();
    let mut stack: Vec<Open<'_>> = Vec::new();
    let close = |open: Open<'_>, stack: &mut Vec<Open<'_>>, ranges: &mut Vec<FoldRange>| {
        if open.last > open.line {
            ranges.push(FoldRange::new(open.line, open.last));
        }
        if let Some(parent) = stack.last_mut() {
            parent.last = parent.last.max(open.last);
        }
    };
    for (index, line) in source.split('\n').enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let what = kind(index, line);
        if what == IndentLine::Skip {
            continue;
        }
        let indent = line.len() - trimmed.len();
        while let Some(top) = stack.last() {
            let inside = indent > top.indent || (indent == top.indent && continues(top.text, line));
            if inside || (what == IndentLine::Text && indent >= top.indent) {
                break;
            }
            let open = stack.pop().expect("just looked at it");
            close(open, &mut stack, &mut ranges);
        }
        if let Some(top) = stack.last_mut() {
            top.last = index;
        }
        if what == IndentLine::Code {
            stack.push(Open {
                indent,
                line: index,
                text: line,
                last: index,
            });
        }
    }
    while let Some(open) = stack.pop() {
        close(open, &mut stack, &mut ranges);
    }
    ranges
}

/// The folds of the brackets in `config` that open and close on different
/// lines, and of block comments over several lines, skipping quoted text
/// (`config.quotes`; a one-character quote ends at the line's end) and line
/// comments. Lines in `skip` (multi-line string bodies the quotes cannot
/// describe, such as heredocs) are ignored. An unclosed bracket folds
/// nothing.
pub fn bracket_fold_ranges(
    source: &str,
    config: &LanguageConfig,
    skip: &[Range<usize>],
) -> Vec<FoldRange> {
    scan_brackets(source, config, skip).ranges
}

/// What [`scan_brackets`] found.
pub(crate) struct BracketScan {
    pub(crate) ranges: Vec<FoldRange>,
    /// Per line, whether it starts outside every bracket, string and
    /// comment.
    pub(crate) at_top: Vec<bool>,
}

/// See [`bracket_fold_ranges`].
pub(crate) fn scan_brackets(
    source: &str,
    config: &LanguageConfig,
    skip: &[Range<usize>],
) -> BracketScan {
    enum Inside<'a> {
        Code,
        Quote(&'a str),
        Block(&'a str, usize),
    }
    let mut ranges = Vec::new();
    let mut at_top = Vec::new();
    // Open brackets: (closer, line).
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut inside = Inside::Code;
    for (index, line) in source.split('\n').enumerate() {
        at_top.push(stack.is_empty() && matches!(inside, Inside::Code));
        if skip.iter().any(|range| range.contains(&index)) {
            continue;
        }
        // A one-character quote does not run past its line.
        if let Inside::Quote(quote) = inside
            && quote.chars().count() == 1
        {
            inside = Inside::Code;
        }
        let first_code = line.len() - line.trim_start().len();
        let mut rest = line;
        let mut at = 0;
        while let Some(character) = rest.chars().next() {
            let step = match inside {
                Inside::Block(end, start) => {
                    if rest.starts_with(end) {
                        if index > start {
                            ranges.push(FoldRange::new(start, index));
                        }
                        inside = Inside::Code;
                        end.len()
                    } else {
                        character.len_utf8()
                    }
                }
                Inside::Quote(quote) => {
                    if character == '\\' && quote.starts_with('"') {
                        rest.chars().take(2).map(char::len_utf8).sum::<usize>()
                    } else if rest.starts_with(quote) {
                        inside = Inside::Code;
                        quote.len()
                    } else {
                        character.len_utf8()
                    }
                }
                Inside::Code => {
                    if config
                        .line_comments
                        .iter()
                        .any(|marker| rest.starts_with(marker))
                    {
                        break;
                    }
                    if let Some((open, end)) = config.block_comment
                        && rest.starts_with(open)
                    {
                        inside = Inside::Block(end, index);
                        open.len()
                    } else if let Some(quote) =
                        config.quotes.iter().find(|quote| rest.starts_with(**quote))
                    {
                        inside = Inside::Quote(quote);
                        quote.len()
                    } else if let Some((_, closer)) =
                        config.brackets.iter().find(|(open, _)| *open == character)
                    {
                        stack.push((*closer, index));
                        1
                    } else if config.brackets.iter().any(|(_, close)| *close == character) {
                        // A stray closer closes nothing; a mismatched one
                        // closes the brackets left open inside it.
                        if let Some(depth) =
                            stack.iter().rposition(|(close, _)| *close == character)
                        {
                            let (_, opened) = stack[depth];
                            stack.truncate(depth);
                            let end = if at == first_code {
                                index.saturating_sub(1)
                            } else {
                                index
                            };
                            if end > opened {
                                ranges.push(FoldRange::new(opened, end));
                            }
                        }
                        1
                    } else {
                        character.len_utf8()
                    }
                }
            };
            let step = step.min(rest.len());
            rest = &rest[step..];
            at += step;
        }
    }
    BracketScan { ranges, at_top }
}

/// Runs of two or more lines that are each only a comment starting with
/// one of `markers`, folded to their first line. Lines in `skip` (string
/// bodies) are never comments.
pub fn comment_fold_ranges(
    source: &str,
    markers: &[&str],
    skip: &[Range<usize>],
) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    let mut flush = |run: &mut Option<(usize, usize)>| {
        if let Some((start, end)) = run.take()
            && end > start
        {
            ranges.push(FoldRange::new(start, end));
        }
    };
    for (index, line) in source.split('\n').enumerate() {
        let trimmed = line.trim_start();
        let comment = !markers.is_empty()
            && markers.iter().any(|marker| trimmed.starts_with(marker))
            && !skip.iter().any(|range| range.contains(&index));
        match (&mut run, comment) {
            (Some((_, end)), true) => *end = index,
            (None, true) => run = Some((index, index)),
            (_, false) => flush(&mut run),
        }
    }
    flush(&mut run);
    ranges
}

/// Multi-line string bodies (see
/// [`ConfigLanguage::string_body_lines`](crate::language::ConfigLanguage::string_body_lines))
/// folded to the line that opens them; a closing delimiter on the line
/// after the body stays visible.
pub fn string_body_fold_ranges(bodies: &[Range<usize>]) -> Vec<FoldRange> {
    bodies
        .iter()
        .filter(|body| body.start > 0 && body.end > body.start)
        .map(|body| FoldRange::new(body.start - 1, body.end - 1))
        .collect()
}

/// What a language gets without its own
/// [`fold_ranges`](crate::language::ConfigLanguage::fold_ranges):
/// indentation, the brackets and comments of its `config`, and its
/// multi-line strings.
pub fn default_fold_ranges(
    source: &str,
    config: &LanguageConfig,
    bodies: &[Range<usize>],
) -> Vec<FoldRange> {
    let comment = |line: &str| {
        let trimmed = line.trim_start();
        config
            .line_comments
            .iter()
            .any(|marker| trimmed.starts_with(marker))
    };
    let in_body = |index: usize| bodies.iter().any(|body| body.contains(&index));
    let mut ranges = indent_fold_ranges(
        source,
        |index, line| {
            if in_body(index) {
                IndentLine::Text
            } else if comment(line) {
                IndentLine::Skip
            } else {
                IndentLine::Code
            }
        },
        |_, _| false,
    );
    if !config.brackets.is_empty() {
        ranges.extend(bracket_fold_ranges(source, config, bodies));
    }
    ranges.extend(comment_fold_ranges(source, config.line_comments, bodies));
    ranges.extend(string_body_fold_ranges(bodies));
    merge_fold_ranges(ranges, source.split('\n').count())
}
