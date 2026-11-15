//! A locked field of a host text that never parsed has no value to pin
//! (or one read off its stand-in): its span is the text found for it (read off the broken text, it may run
//! past the field). The edit making the text parse may close it: it only
//! inserts, on the field's lines, what closes what the span leaves open
//! (the language's closers of its open containers, a quote it holds an odd
//! number of, the end of markup it leaves open) or a separator. No other
//! text changes, so the field reads as the host wrote it; anything else
//! changes the span, as any edit does.

use super::super::super::language::LanguageConfig;
use super::super::locate::lookup;
use super::super::locks::Lock;
use super::edit::Edited;
use super::stretch::is_blank;

/// Whether `edit`, making a text that did not parse parse, only closes
/// `lock`, which then reads as pinned when a value is.
pub(super) fn only_closes(edit: &Edited<'_>, lock: &Lock) -> bool {
    let Some(pointer) = lock.pointer.as_deref() else {
        return false;
    };
    if edit.protected.document.is_some() {
        return false;
    }
    let Some(edited) = edit.parsed() else {
        return false;
    };
    // A value pinned (from the stand-in) must read as pinned.
    let pin = edit
        .protected
        .pins
        .iter()
        .find(|(known, _)| known == pointer);
    if pin.is_some_and(|(_, value)| lookup(edited, pointer) != Some(value)) {
        return false;
    }
    // Only an insertion, on the field's lines.
    let removed = &edit.old[edit.region.clone()];
    if removed.iter().any(|unit| !is_blank(*unit)) || !on_lines(edit, lock) {
        return false;
    }
    // The span up to where the closer goes (read off a broken text, it may
    // run on past the field: its document's own closer is not its).
    let written: String = lock
        .ranges
        .iter()
        .filter(|range| range.start < edit.region.start)
        .map(|range| {
            String::from_utf16_lossy(&edit.old[range.start..range.end.min(edit.region.start)])
        })
        .collect();
    // What closes the containers the span leaves open (closing brackets,
    // XML end tags), each taken out of what was inserted once.
    let mut rest: String = String::from_utf16_lossy(&edit.new[edit.new_region.clone()])
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    for closer in edit.behavior.container_closers(&written) {
        rest = rest.replacen(&closer, "", 1);
    }
    let config = edit.behavior.config();
    let markup = edit.behavior.markup_delimiters();
    rest.chars()
        .all(|character| closes(&config, markup, &written, character))
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

/// Whether the edit's region lies on the lines of `lock`'s stretches.
fn on_lines(edit: &Edited<'_>, lock: &Lock) -> bool {
    let lines = &edit.lines;
    let region = &edit.region;
    lock.ranges.iter().any(|range| {
        let first = lines.line_bounds(lines.line_of(range.start)).start;
        let last = lines
            .line_bounds(lines.line_of(range.end.saturating_sub(1).max(range.start)))
            .end;
        first <= region.start && region.end <= last + 1
    })
}
