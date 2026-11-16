//! A text that does not parse, for the locks the text before cannot carry
//! into it (the host gave it so, or named a lock while it was so), read
//! two ways:
//! - `stand_in`: the same text with the line breaking it blanked, when that
//!   parses: its locks fall on the same places, and its values are pinned;
//! - `written`: each line's path, read by the tolerant path lookup the
//!   caret uses, so the schema's read-only fields and the contents of a
//!   locked block are found where the stand-in has none (in a large text,
//!   the lines naming a protected field and the block under each).
//!
//! `read_off` places a field from these readings, also in a text that does
//! not parse after one that did, for a field with no lock to follow there
//! (a path named while it does not parse). A lock placed so is followed
//! through edits by position (see `locks`), never read again off the
//! lines; once the text parses, the parse places it.

use serde_json::Value;

use super::super::applies::Applies;
use super::super::codec;
use super::super::language::{ConfigLanguage, Repetition};
use super::super::paths::{FieldPath, field_range, inside, key_range, path_at};
use super::locate::{Extent, LineIndex, within};
use super::marks::read_only_in;
use super::texts::Reason;

/// Lines of a text read line by line at most (each line's path is read
/// from the top); a larger one is read on the lines `candidates` picks.
const MAX_LINES: usize = 500;

/// Line reads a large text gets (each costs a read from the top, so they
/// are this divided by its lines), as many as a text of `MAX_LINES` lines
/// gets for every line.
const LARGE_READS: usize = MAX_LINES * MAX_LINES / 2;

/// Lines tried blanked (their last character, then all of it) in each
/// round at most: the one the parser stopped
/// at, then those above it (an unclosed bracket or quote is found below
/// where it is written), then the one below.
const STAND_IN_TRIES: usize = 24;

/// Lines blanked at most (one per break).
const STAND_IN_ROUNDS: usize = 4;

/// `text` with the lines breaking it blanked (each character a space of
/// the same UTF-16 length, so every other place keeps its offsets) such
/// that it parses: in each round, a line at or above where the parser
/// stopped, the one that parses, else the one taking the parser furthest.
pub(super) fn stand_in(
    behavior: &dyn ConfigLanguage,
    text: &str,
    error_line: Option<usize>,
    reading_schema: Option<&Value>,
) -> Option<StandIn> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let last = lines.len().saturating_sub(1);
    let mut error_line = error_line;
    let mut blanked_lines = Vec::new();
    for _ in 0..STAND_IN_ROUNDS {
        let stop = error_line.map_or(last, |line| line.saturating_sub(1).min(last));
        let tries = (0..=stop)
            .rev()
            .take(STAND_IN_TRIES - 1)
            .chain((stop < last).then_some(stop + 1));
        // The furthest the parser got with one line blanked.
        let mut best: Option<(usize, String, usize)> = None;
        for line in tries {
            if lines[line].trim().is_empty() {
                continue;
            }
            // Its last character alone (a trailing comma), else all of it.
            for blanked_line in [blank_last(&lines[line]), blank(&lines[line])] {
                let kept = std::mem::replace(&mut lines[line], blanked_line.clone());
                let blanked = lines.join("\n");
                lines[line] = kept;
                match codec::read_aside(behavior, &blanked, reading_schema) {
                    Ok(_) => {
                        blanked_lines.push(line);
                        return Some(StandIn {
                            text: blanked,
                            blanked: blanked_lines,
                        });
                    }
                    Err(error) => {
                        let reached = error.line.unwrap_or(0);
                        if reached > error_line.unwrap_or(0)
                            && best.as_ref().is_none_or(|(_, _, at)| reached > *at)
                        {
                            best = Some((line, blanked_line, reached));
                        }
                    }
                }
            }
        }
        let (line, blanked_line, reached) = best?;
        lines[line] = blanked_line;
        blanked_lines.push(line);
        error_line = Some(reached);
    }
    None
}

/// A text that does not parse with the lines breaking it blanked.
pub(super) struct StandIn {
    pub(super) text: String,
    /// The zero-based lines blanked.
    pub(super) blanked: Vec<usize>,
}

/// `line` with its last non-blank character a space (or two, for one of
/// two UTF-16 units).
fn blank_last(line: &str) -> String {
    let trimmed = line.trim_end();
    let mut kept: String = trimmed
        .chars()
        .rev()
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if let Some(last) = trimmed.chars().next_back() {
        kept.push_str(&" ".repeat(last.len_utf16()));
    }
    kept.push_str(&line[trimmed.len()..]);
    kept
}

/// `line` as spaces of the same UTF-16 length.
fn blank(line: &str) -> String {
    line.chars()
        .map(|character| " ".repeat(character.len_utf16()))
        .collect()
}

/// One line holding code, and the path of what starts it.
#[derive(Clone)]
pub(super) struct Written {
    pub(super) line: usize,
    pub(super) pointer: String,
}

/// The path at the start of each line of `text` holding code (not blank,
/// not only a comment): of the lines `only`, else of every line, or in a
/// large text of the lines naming one of `names` (see `candidates`).
pub(super) fn written(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    only: Option<&[usize]>,
    names: &[String],
) -> Vec<Written> {
    let count = lines.line_count();
    let chosen: Vec<usize> = match only {
        Some(only) => only.to_vec(),
        None if count > MAX_LINES => {
            let names_one =
                |content: &str| names.iter().any(|name| content.contains(name.as_str()));
            candidates(behavior, lines, &names_one, LARGE_READS / count)
        }
        None => (0..count).collect(),
    };
    let mut found = Vec::new();
    for line in chosen {
        let content = lines.line_text(line);
        let indent = content.len() - content.trim_start().len();
        if content.trim().is_empty() || behavior.comment_start(content) == Some(indent) {
            continue;
        }
        let at = lines.line_bounds(line).start + content[..indent].encode_utf16().count();
        if let Some(path) = path_at(behavior, text, at) {
            found.push(Written {
                line,
                pointer: path.to_pointer(),
            });
        }
    }
    found
}

/// Up to `budget` lines of a large text that may hold a protected field:
/// each line `names_one` accepts (naming a protected field's key), with the
/// lines indented under it, or up to the next section after a line that
/// starts one (TOML's `[limits]`): its block.
pub(super) fn candidates(
    behavior: &dyn ConfigLanguage,
    lines: &LineIndex<'_>,
    names_one: &dyn Fn(&str) -> bool,
    budget: usize,
) -> Vec<usize> {
    let indent = |text: &str| text.len() - text.trim_start().len();
    let header = |text: &str| behavior.starts_section(text);
    let mut chosen = Vec::new();
    let mut line = 0;
    while line < lines.line_count() && chosen.len() < budget {
        let content = lines.line_text(line);
        line += 1;
        if !names_one(content) {
            continue;
        }
        chosen.push(line - 1);
        let own = indent(content);
        let opens_section = header(content);
        while line < lines.line_count() && chosen.len() < budget {
            let next = lines.line_text(line);
            if !next.trim().is_empty() {
                let inside = if opens_section {
                    !header(next)
                } else {
                    indent(next) > own
                };
                if !inside {
                    break;
                }
                chosen.push(line);
            }
            line += 1;
        }
    }
    chosen
}

/// Whether `line` names one of `names` as a word (`"id"`, `<id>`, `id:`,
/// not `valid`).
pub(super) fn names_word(names: &[String], line: &str) -> bool {
    let word = |character: char| character.is_alphanumeric() || matches!(character, '_' | '-');
    names.iter().any(|name| {
        !name.is_empty()
            && line.match_indices(name.as_str()).any(|(at, _)| {
                !line[..at].chars().next_back().is_some_and(word)
                    && !line[at + name.len()..].chars().next().is_some_and(word)
            })
    })
}

/// The fields `schema` marks read-only among those `written`, outermost
/// only, with their reasons.
pub(super) fn marked(
    behavior: &dyn ConfigLanguage,
    schema: &Value,
    written: &[Written],
) -> Vec<(String, Reason)> {
    let repeats = behavior.repetition() != Repetition::None;
    let applies = Applies::blind(schema, repeats);
    let mut marked: Vec<(String, Reason)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for field in written {
        if seen.contains(&field.pointer.as_str()) {
            continue;
        }
        seen.push(&field.pointer);
        let path = FieldPath::from(field.pointer.as_str());
        let segments = path.segments();
        for depth in 1..=segments.len() {
            let branches = applies.at(&segments[..depth]).branches();
            if let Some(reason) = read_only_in(&branches) {
                let pointer = FieldPath::from(segments[..depth].to_vec()).to_pointer();
                if !marked.iter().any(|(known, _)| *known == pointer) {
                    marked.push((pointer, reason.map_or(Reason::Field, Reason::Given)));
                }
                break;
            }
        }
    }
    marked
}

/// Where the field at `pointer` seems to be written when its path finds
/// nothing: the first line whose path ends in the field's own key, with
/// the lines `written` inside that.
pub(super) fn under_own_key(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    written: &[Written],
    pointer: &str,
) -> Option<Extent> {
    let key = pointer.rsplit('/').next().filter(|key| !key.is_empty())?;
    let found = written
        .iter()
        .find(|field| field.pointer != pointer && field.pointer.rsplit('/').next() == Some(key))?;
    read_off(behavior, text, lines, None, written, &found.pointer).or_else(|| {
        let content = lines.line_text(found.line);
        let indent = content.len() - content.trim_start().len();
        let start = lines.line_bounds(found.line).start + content[..indent].encode_utf16().count();
        let end = lines.line_bounds(found.line).start + content.trim_end().encode_utf16().count();
        Some(Extent {
            key: None,
            ranges: std::iter::once(start..end).collect(),
        })
    })
}

/// Where the field at `pointer` is written in a text that does not parse,
/// read off its lines: where the language's tolerant lookup finds it
/// (`field_range`), with what is inside it: the fields of `value` (its
/// value pinned in the last text that parsed) where the lookup finds them,
/// else the lines `written` inside it; joined where only its own lines lie
/// between them, the last carried over the closers of the containers it
/// leaves open.
pub(super) fn read_off(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    value: Option<&Value>,
    written: &[Written],
    pointer: &str,
) -> Option<Extent> {
    let mut ranges = vec![lines.range(field_range(behavior, text, pointer)?)];
    let key = key_range(behavior, text, pointer).map(|range| lines.range(range));
    if let Some(value) = value {
        ranges.extend(
            inside(value, pointer)
                .iter()
                .filter_map(|child| field_range(behavior, text, child))
                .map(|range| lines.range(range)),
        );
    }
    for field in written.iter().filter(|_| value.is_none()) {
        if field.pointer != pointer && within(behavior, &field.pointer, pointer) {
            let bounds = lines.line_bounds(field.line);
            let content = lines.line_text(field.line);
            let code = content.trim();
            let indent = content.len() - content.trim_start().len();
            let start = bounds.start + content[..indent].encode_utf16().count();
            let code = match behavior.config().item_separator {
                Some(separator) => code.strip_suffix(separator).unwrap_or(code).trim_end(),
                None => code,
            };
            ranges.push(start..start + code.encode_utf16().count());
        }
    }
    if let Some(first) = ranges.first_mut() {
        let name = lines.byte_of(first.start);
        if let Some(open) = super::super::flow_syntax::markup_before(behavior, text, name) {
            first.start -= text[open..name].encode_utf16().count();
        }
    }
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(last)
                if range.start <= last.end
                    || own_lines_between(behavior, text, lines, last.end..range.start, pointer) =>
            {
                last.end = last.end.max(range.end);
            }
            _ => merged.push(range),
        }
    }
    if let (Some(first), Some(last)) = (merged.first().cloned(), merged.last_mut()) {
        let covered = &text[lines.byte_of(first.start)..lines.byte_of(last.end)];
        let mut depth = behavior.unclosed_containers(covered);
        let mut end = lines.byte_of(last.end);
        while depth > 0 {
            let rest = &text[end..];
            let skipped = rest.len() - rest.trim_start().len();
            let Some(length) = behavior.container_closer_len(&rest[skipped..]) else {
                break;
            };
            end += skipped + length;
            depth -= 1;
        }
        last.end = lines.units_of(end);
    }
    Some(Extent {
        key,
        ranges: merged,
    })
}

/// Whether the lines wholly inside `gap` are blank, comments, punctuation
/// alone, or start with a path inside `pointer`'s.
fn own_lines_between(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    gap: std::ops::Range<usize>,
    pointer: &str,
) -> bool {
    let punctuation = |character: char| {
        character.is_whitespace()
            || (character.is_ascii_punctuation() && !matches!(character, '"' | '\''))
    };
    (lines.line_of(gap.start) + 1..lines.line_of(gap.end)).all(|line| {
        let content = lines.line_text(line);
        let indent = content.len() - content.trim_start().len();
        if content.trim().chars().all(punctuation)
            || behavior.comment_start(content) == Some(indent)
        {
            return true;
        }
        let at = lines.line_bounds(line).start + content[..indent].encode_utf16().count();
        path_at(behavior, text, at)
            .is_some_and(|path| within(behavior, &path.to_pointer(), pointer))
    })
}
