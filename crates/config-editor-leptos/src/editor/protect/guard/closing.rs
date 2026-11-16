//! A locked field of a host text that never parsed has no value pinned: its
//! span is the text read for it off the broken text, which may run past
//! the field or stop short of its value, so its text says nothing sure.
//! The edit making the text parse may close it: it only inserts, on the
//! field's lines, what closes what the span leaves open (the language's
//! closers of its open containers, a quote it holds an odd number of, the
//! end of markup it leaves open) or a separator. No other text changes, so
//! the field reads as the host wrote it.

use std::ops::Range;

use serde_json::Value;

use super::super::super::language::{ConfigLanguage, LanguageConfig};
use super::super::locate::{LineIndex, lookup};
use super::super::locks::Lock;
use super::stretch::is_blank;

/// The edit as the closing check reads it: the two texts, and the region
/// it replaced in each (UTF-16).
pub(super) struct Closing<'a> {
    pub(super) behavior: &'a dyn ConfigLanguage,
    pub(super) before: &'a str,
    pub(super) old: &'a [u16],
    pub(super) new: &'a [u16],
    pub(super) region: Range<usize>,
    pub(super) new_region: Range<usize>,
}

impl Closing<'_> {
    /// Whether the edit, making the text parse as `edited`, only closes
    /// `lock` (no value pinned, or `pin` from a stand-in, which it must then
    /// read as).
    pub(super) fn only_closes(&self, lock: &Lock, pin: Option<&Value>, edited: &Value) -> bool {
        let Some(pointer) = lock.pointer.as_deref() else {
            return false;
        };
        if pin.is_some_and(|value| lookup(edited, pointer) != Some(value)) {
            return false;
        }
        // Only an insertion, on the field's lines.
        let removed = &self.old[self.region.clone()];
        if removed.iter().any(|unit| !is_blank(*unit)) || !self.on_lines(lock) {
            return false;
        }
        // The span up to where the closer goes (read off a broken text, it
        // may run on past the field: its document's own closer is not its).
        let written: String = lock
            .ranges
            .iter()
            .filter(|range| range.start < self.region.start)
            .map(|range| {
                String::from_utf16_lossy(&self.old[range.start..range.end.min(self.region.start)])
            })
            .collect();
        // What closes the containers the span leaves open (closing brackets,
        // XML end tags), each taken out of what was inserted once.
        let mut rest: String = String::from_utf16_lossy(&self.new[self.new_region.clone()])
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        for closer in self.behavior.container_closers(&written) {
            rest = rest.replacen(&closer, "", 1);
        }
        let config = self.behavior.config();
        let markup = self.behavior.markup_delimiters();
        rest.chars()
            .all(|character| closes(&config, markup, &written, character))
    }

    /// Whether the edit's region lies on the lines of `lock`'s stretches.
    fn on_lines(&self, lock: &Lock) -> bool {
        let lines = LineIndex::new(self.before);
        let region = &self.region;
        lock.ranges.iter().any(|range| {
            let first = lines.line_bounds(lines.line_of(range.start)).start;
            let last = lines
                .line_bounds(lines.line_of(range.end.saturating_sub(1).max(range.start)))
                .end;
            first <= region.start && region.end <= last + 1
        })
    }
}

/// Whether `character`, inserted, closes a quote or markup `written`
/// leaves open, or separates a key from its value.
fn closes(
    config: &LanguageConfig,
    markup: Option<(char, char)>,
    written: &str,
    character: char,
) -> bool {
    if config
        .separators
        .iter()
        .any(|separator| separator.contains(character))
    {
        return true;
    }
    if let Some((open, close)) = markup
        && character == close
    {
        return written.matches(open).count() > written.matches(close).count();
    }
    config
        .quotes
        .iter()
        .any(|quote| quote.starts_with(character))
        && written.matches(character).count() % 2 == 1
}
