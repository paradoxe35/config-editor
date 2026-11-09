//! Finding a query in the document: plain text or a regular expression,
//! optionally matching case and whole words. Pure, so it is tested natively.

use std::ops::Range;

use crate::editor::pattern_engine::{Regex, RegexBuilder};

/// Most matches found; past it the count reads "10000+" and later matches
/// are neither highlighted nor stepped to (replace all still replaces them).
pub(in crate::editor) const MAX_MATCHES: usize = 10_000;

/// Largest compiled pattern, so a pathological one is an error, not a stall.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// How the query is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::editor) struct SearchOptions {
    pub(in crate::editor) case_sensitive: bool,
    pub(in crate::editor) whole_word: bool,
    pub(in crate::editor) regex: bool,
}

/// One match, as byte offsets into the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) struct SearchMatch {
    pub(in crate::editor) start: usize,
    pub(in crate::editor) end: usize,
    /// Zero-based line of `start`.
    pub(in crate::editor) line: usize,
    /// Byte offset where that line starts.
    pub(in crate::editor) line_start: usize,
}

/// A compiled query.
pub(in crate::editor) struct Matcher {
    kind: Kind,
    whole_word: bool,
}

enum Kind {
    /// Case-sensitive text: the standard library's substring search.
    Exact(String),
    /// Text in any case: each character compared by its simple lowercase.
    AnyCase(Vec<char>),
    Pattern(Regex),
}

impl Matcher {
    /// The matcher for `query`, `None` for an empty one, or the pattern's
    /// error message.
    pub(in crate::editor) fn new(
        query: &str,
        options: SearchOptions,
    ) -> Result<Option<Self>, String> {
        if query.is_empty() {
            return Ok(None);
        }
        let kind = if options.regex {
            let regex = RegexBuilder::new(query)
                .case_insensitive(!options.case_sensitive)
                .multi_line(true)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|error| first_line(&error.to_string()))?;
            Kind::Pattern(regex)
        } else if options.case_sensitive {
            Kind::Exact(query.to_string())
        } else {
            Kind::AnyCase(query.chars().map(fold).collect())
        };
        Ok(Some(Self {
            kind,
            whole_word: options.whole_word,
        }))
    }

    /// The first match starting at or after byte `from`. A pattern may match
    /// empty text (`^`, `$`, `x*`): it is found where it matches, as in VS
    /// Code, and [`after`](Self::after) steps past it.
    pub(in crate::editor) fn find_from(&self, text: &str, mut from: usize) -> Option<Range<usize>> {
        while from <= text.len() {
            let found = self.raw_find(text, from)?;
            if !self.whole_word || is_whole_word(text, &found) {
                return Some(found);
            }
            // Part of a word: look again one character on.
            from = found.start + text[found.start..].chars().next().map_or(1, char::len_utf8);
        }
        None
    }

    /// Where the search for the match after `found` starts: its end, or
    /// one character on from an empty match (past the end of `text` after
    /// one there), so every loop over the matches ends.
    pub(in crate::editor) fn after(text: &str, found: &Range<usize>) -> usize {
        if found.is_empty() {
            found.end + text[found.end..].chars().next().map_or(1, char::len_utf8)
        } else {
            found.end
        }
    }

    fn raw_find(&self, text: &str, from: usize) -> Option<Range<usize>> {
        match &self.kind {
            Kind::Exact(needle) => text[from..]
                .find(needle.as_str())
                .map(|start| from + start..from + start + needle.len()),
            Kind::AnyCase(needle) => find_any_case(text, needle, from),
            Kind::Pattern(regex) => regex.find_at(text, from).map(|found| found.range()),
        }
    }

    /// What replaces the match at `range`: `replacement` as written, or for
    /// a regular expression with its groups and escapes filled in as VS Code
    /// does (see `template`).
    pub(in crate::editor) fn replacement_for(
        &self,
        text: &str,
        range: &Range<usize>,
        replacement: &str,
    ) -> String {
        let Kind::Pattern(regex) = &self.kind else {
            return replacement.to_string();
        };
        match regex.captures_at(text, range.start) {
            Some(captures) if captures.get(0).map(|whole| whole.range()) == Some(range.clone()) => {
                super::template::expand(replacement, regex, &captures)
            }
            _ => replacement.to_string(),
        }
    }
}

/// Every match of `matcher` in `text`, in order, at most [`MAX_MATCHES`];
/// the flag says whether more were left out.
pub(in crate::editor) fn find_matches(text: &str, matcher: &Matcher) -> (Vec<SearchMatch>, bool) {
    let mut matches = Vec::new();
    let (mut line, mut line_start, mut scanned) = (0, 0, 0);
    let mut from = 0;
    while let Some(found) = matcher.find_from(text, from) {
        if matches.len() == MAX_MATCHES {
            return (matches, true);
        }
        let between = &text[scanned..found.start];
        line += between.bytes().filter(|byte| *byte == b'\n').count();
        if let Some(newline) = between.rfind('\n') {
            line_start = scanned + newline + 1;
        }
        scanned = found.start;
        matches.push(SearchMatch {
            start: found.start,
            end: found.end,
            line,
            line_start,
        });
        from = Matcher::after(text, &found);
    }
    (matches, false)
}

/// A character's simple lowercase, or the character when that is several.
fn fold(character: char) -> char {
    if character.is_ascii() {
        return character.to_ascii_lowercase();
    }
    let mut lower = character.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

fn find_any_case(text: &str, needle: &[char], from: usize) -> Option<Range<usize>> {
    let first = *needle.first()?;
    for (offset, character) in text[from..].char_indices() {
        if fold(character) != first {
            continue;
        }
        let start = from + offset;
        let mut rest = text[start..].char_indices();
        let mut end = start;
        let matched = needle.iter().all(|wanted| match rest.next() {
            Some((at, character)) if fold(character) == *wanted => {
                end = start + at + character.len_utf8();
                true
            }
            _ => false,
        });
        if matched {
            return Some(start..end);
        }
    }
    None
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Whether `range` neither starts nor ends inside a word.
fn is_whole_word(text: &str, range: &Range<usize>) -> bool {
    let found = &text[range.clone()];
    let joins = |outside: Option<char>, inside: Option<char>| {
        outside.is_some_and(is_word_character) && inside.is_some_and(is_word_character)
    };
    !joins(
        text[..range.start].chars().next_back(),
        found.chars().next(),
    ) && !joins(text[range.end..].chars().next(), found.chars().next_back())
}

/// Regex errors span several lines (the pattern, a caret, the message);
/// the widget has room for the message.
fn first_line(message: &str) -> String {
    message
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with("error:"))
        .map(|line| line.trim_start().trim_start_matches("error:").trim())
        .or_else(|| message.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("invalid pattern")
        .to_string()
}
