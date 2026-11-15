//! One edit as the checks read it: the two texts as UTF-16 units, what
//! they share at their ends, the region the edit replaced, and the edited
//! text parsed (at most once, when a check asks for it).

use std::cell::OnceCell;
use std::ops::Range;

use serde_json::Value;

use super::super::super::codec;
use super::super::super::diff::{DiffEdit, myers_edits};
use super::super::super::language::ConfigLanguage;
use super::super::locate::{LineIndex, values_under};
use super::super::locks::{Lock, Protected};
use super::super::texts::Reason;
use super::content::{content_areas, in_locked_content};
use super::pins::{all_pinned, pins_hold};
use super::spacing::between_tokens;
use super::stretch::{is_blank, kept};

/// The edit turning `before` (whose locks are `protected`) into `after`.
pub(super) struct Edited<'a> {
    pub(super) behavior: &'a dyn ConfigLanguage,
    pub(super) reading_schema: Option<&'a Value>,
    pub(super) protected: &'a Protected,
    pub(super) before: &'a str,
    pub(super) after: &'a str,
    pub(super) old: Vec<u16>,
    pub(super) new: Vec<u16>,
    /// Units the two texts share at their start and end (these may
    /// overlap).
    pub(super) prefix: usize,
    pub(super) suffix: usize,
    /// The lines of `before`.
    pub(super) lines: LineIndex<'a>,
    /// The region the edit replaced, in `before` and in `after`: where the
    /// caret it left places it (inserted text ends there), else as narrowly
    /// as the texts allow. Text inserted beside a copy of itself (a locked
    /// line pasted above the lock) could be read anywhere along the copy,
    /// into the lock; the caret tells where it went.
    pub(super) region: Range<usize>,
    pub(super) new_region: Range<usize>,
    /// Whether the caret placed `region` (else a diff of the texts did).
    placed: bool,
    parsed: OnceCell<Option<Value>>,
    unchanged: OnceCell<Option<[Vec<bool>; 2]>>,
    reformat: OnceCell<bool>,
    spacing: OnceCell<bool>,
    content: OnceCell<Vec<Range<usize>>>,
    pinned: OnceCell<bool>,
}

impl<'a> Edited<'a> {
    pub(super) fn new(
        behavior: &'a dyn ConfigLanguage,
        reading_schema: Option<&'a Value>,
        protected: &'a Protected,
        before: &'a str,
        after: &'a str,
        caret: Option<usize>,
    ) -> Self {
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let mut prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let mut suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let mut placed = false;
        if caret.is_none() {
            let tail = suffix.min(old.len() - prefix).min(new.len() - prefix);
            let (slid, kept) = super::super::carry::slid_to_line_start(&old, &new, prefix, tail);
            // Only when it slid: the texts' whole common ends are kept
            // otherwise (they may overlap).
            if slid != prefix {
                (prefix, suffix) = (slid, kept);
            }
        }
        let lines = LineIndex::new(before);
        let tail = suffix
            .min(old.len() - prefix.min(old.len()))
            .min(new.len() - prefix.min(new.len()));
        let mut region = prefix.min(old.len())..old.len() - tail;
        let mut new_region = prefix.min(new.len())..new.len() - tail;
        // Placed by the caret: the texts agree before the inserted text
        // starts and after it ends. The texts kept around it are then those
        // before and after that place (one of two like lines deleted: the
        // one the caret says, which the host's ranges follow too).
        if let Some(from) = caret.and_then(|caret| {
            super::super::carry::placed_by_caret(&old, &new, prefix, suffix, caret)
        }) {
            region = from..from + region.len();
            new_region = from..from + new_region.len();
            prefix = from;
            suffix = old.len() - region.end;
            placed = true;
        }
        Edited {
            behavior,
            reading_schema,
            protected,
            before,
            after,
            old,
            new,
            prefix,
            suffix,
            lines,
            region,
            new_region,
            placed,
            parsed: OnceCell::new(),
            unchanged: OnceCell::new(),
            reformat: OnceCell::new(),
            spacing: OnceCell::new(),
            content: OnceCell::new(),
            pinned: OnceCell::new(),
        }
    }

    /// The edited text parsed, when it parses.
    pub(super) fn parsed(&self) -> Option<&Value> {
        self.parsed
            .get_or_init(|| codec::read(self.behavior, self.after, self.reading_schema).ok())
            .as_ref()
    }

    /// Whether `range` of `before` survives the edit: its text is kept
    /// with nothing glued onto its ends, or it is on lines an edit
    /// changing several places (Replace All, a comment toggle over lines)
    /// left whole. An edit made inside it, by position, never keeps it,
    /// though a copy of its line elsewhere lets a line diff take that copy
    /// for it.
    pub(super) fn kept(&self, range: &Range<usize>) -> bool {
        let inside = range.start < self.region.start && self.region.end < range.end;
        // Its text taken away whole and not written again (one of two like
        // lines deleted where the caret says): no line diff keeps it.
        // (Only where the caret placed the edit: a diff of the texts alone
        // may say any of the alike stretches went.)
        let covered =
            self.placed && self.region.start <= range.start && range.end <= self.region.end;
        if covered && !range.is_empty() {
            let text = &self.old[range.clone()];
            let written = &self.new[self.new_region.clone()];
            if !written.windows(text.len()).any(|window| window == text) {
                return false;
            }
        }
        kept(
            range,
            &self.old,
            &self.new,
            self.prefix,
            self.suffix,
            &|| self.strict(),
        ) || (!inside && self.lines_kept(range))
    }

    /// Whether the lines of `range` of `before` are kept whole by a line
    /// diff of the two texts, either one of the two shortest (two lines
    /// swapped keep one or the other: the edit and its undo, a line moved
    /// up or down past a locked one, are judged alike).
    pub(super) fn lines_kept(&self, range: &Range<usize>) -> bool {
        self.kept_by_one_diff(std::slice::from_ref(range))
    }

    /// Whether every one of `ranges` of `before` survives the edit in the
    /// same reading of it: each kept as text, or all the others' lines
    /// kept whole by one line diff. A locked value and the keys holding it
    /// are judged together: two lines swapped keep one or the other, not
    /// both (`db:` moved below its `image: pg`).
    pub(super) fn kept_together(&self, ranges: &[Range<usize>]) -> bool {
        let moved: Vec<Range<usize>> = ranges
            .iter()
            .filter(|range| {
                !kept(
                    range,
                    &self.old,
                    &self.new,
                    self.prefix,
                    self.suffix,
                    &|| self.strict(),
                )
            })
            .cloned()
            .collect();
        moved.is_empty() || self.kept_by_one_diff(&moved)
    }

    /// Whether one line diff of the two texts keeps the lines of all of
    /// `ranges` whole.
    fn kept_by_one_diff(&self, ranges: &[Range<usize>]) -> bool {
        let lines = &self.lines;
        // A range ending with its line's break holds the break too: the
        // line after it is kept as well, or the break may be gone (the
        // last line cut, joining it onto the end of the text).
        let wanted = || {
            ranges.iter().flat_map(|range| {
                let ends_line =
                    range.end > range.start && self.old[range.end - 1] == u16::from(b'\n');
                let last = lines.line_of(range.end.saturating_sub(1)) + usize::from(ends_line);
                lines.line_of(range.start)..=last
            })
        };
        self.unchanged
            .get_or_init(|| unchanged_lines(self.before, self.after))
            .as_ref()
            .is_some_and(|diffs| {
                diffs
                    .iter()
                    .any(|same| wanted().all(|line| same.get(line).copied().unwrap_or(false)))
            })
    }

    /// Whether the text parses before and after, and the lines of `lock`
    /// are in the text after once, whole: moved there (past several lines,
    /// which no line diff keeps them through), its value then checked as
    /// every pinned one is.
    pub(super) fn moved_whole(&self, lock: &Lock) -> bool {
        if self.protected.document.is_none() {
            return false;
        }
        let starts = lock.key.iter().chain(&lock.ranges).map(|range| range.start);
        let ends = lock.key.iter().chain(&lock.ranges).map(|range| range.end);
        let (Some(low), Some(high)) = (starts.min(), ends.max()) else {
            return false;
        };
        let lines = &self.lines;
        let start = lines.line_bounds(lines.line_of(low)).start;
        let end = lines
            .line_bounds(lines.line_of(high.saturating_sub(1).max(low)))
            .end;
        let block = String::from_utf16_lossy(&self.old[start..end]);
        let padded = format!("\n{}\n", self.after);
        padded.matches(&format!("\n{block}\n")).count() == 1
            && self.parsed().is_some_and(|document| {
                pins_hold(document, self.protected).is_none() && self.items_kept(lock, document)
            })
    }

    /// Whether a list item a host range covers is still read in `document`
    /// (the text after) under its list as often as before: its pointer's
    /// index moves with the items around it, so its pin, read at the
    /// pointer, may be another item's, and the lines kept whole may be a
    /// copy of its line written where no list reads it (the item's own line
    /// deleted below a copy). Each value its list read before is read there
    /// as often again.
    fn items_kept(&self, lock: &Lock, document: &Value) -> bool {
        let Some(pointer) = lock
            .pointer
            .as_deref()
            .filter(|_| lock.reason == Reason::Range)
        else {
            return true;
        };
        let keys: Vec<&str> = pointer
            .split('/')
            .filter(|segment| !segment.is_empty() && segment.parse::<usize>().is_err())
            .collect();
        if keys.len()
            == pointer
                .split('/')
                .filter(|segment| !segment.is_empty())
                .count()
        {
            return true;
        }
        let Some(before) = self.protected.document.as_deref() else {
            return true;
        };
        let (mut old, mut new) = (Vec::new(), Vec::new());
        values_under(before, &keys, &mut old);
        values_under(document, &keys, &mut new);
        old.iter().all(|value| {
            let count = |values: &[&Value]| values.iter().filter(|other| *other == value).count();
            count(&new) >= count(&old)
        })
    }

    /// Whether only whitespace changed inside a locked field (an indent, a
    /// space after `:`), which leaves every locked value as it was.
    pub(super) fn reformat_only(&self) -> bool {
        *self.reformat.get_or_init(|| {
            without_whitespace(&self.old[self.region.clone()])
                == without_whitespace(&self.new[self.new_region.clone()])
                && self
                    .parsed()
                    .is_some_and(|document| pins_hold(document, self.protected).is_none())
                && self.all_pinned()
                && (!self.behavior.indent_is_structure()
                    || indents(self.behavior, self.before) == indents(self.behavior, self.after))
        })
    }

    /// Whether the edit, made on a text that does not parse afterwards,
    /// changes only whitespace between tokens (see `spacing`): formatting,
    /// free around and inside locked fields as it is while the text parses.
    pub(super) fn spacing_only(&self) -> bool {
        *self.spacing.get_or_init(|| {
            // Read first, as it costs nothing: more than whitespace changed
            // (a key typed into a locked value) needs no parse to tell.
            let whitespace_only = without_whitespace(&self.old[self.region.clone()])
                == without_whitespace(&self.new[self.new_region.clone()]);
            if !whitespace_only || self.parsed().is_some() {
                return false;
            }
            let lines = &self.lines;
            let start = lines.line_bounds(lines.line_of(self.region.start)).start;
            let end = lines.line_bounds(lines.line_of(self.region.end)).end;
            let new_end = end + self.new.len() - self.old.len();
            if new_end < self.new_region.end || end < self.region.end {
                return false;
            }
            let old = String::from_utf16_lossy(&self.old[start..end]);
            // Each character's UTF-16 offset in the text before.
            let offsets: Vec<usize> = old
                .chars()
                .scan(start, |at, character| {
                    let here = *at;
                    *at += character.len_utf16();
                    Some(here)
                })
                .chain([end])
                .collect();
            let locks = &self.protected.locks;
            let content = |from: usize, to: usize| {
                in_locked_content(self.content_areas(), locks, offsets[from], offsets[to])
            };
            between_tokens(
                self.behavior,
                &old,
                &String::from_utf16_lossy(&self.new[start..new_end]),
                &content,
            )
        })
    }

    /// Where whatever is typed in the text before is content (see
    /// `content`), read once.
    pub(super) fn content_areas(&self) -> &[Range<usize>] {
        self.content
            .get_or_init(|| content_areas(self.behavior, self.before, &self.lines))
    }

    /// Whether every locked field has a value pinned, read once.
    fn all_pinned(&self) -> bool {
        *self.pinned.get_or_init(|| all_pinned(self.protected))
    }

    /// Without a parse to compare, a character typed next to a YAML plain
    /// value joins it, punctuation included (`abc<`, `2[x]`); so it does in
    /// any language next to a lock with no pinned value (`2/ 3` in HCL).
    fn strict(&self) -> bool {
        (self.behavior.plain_scalars() && self.parsed().is_none()) || !self.all_pinned()
    }
}

/// For each line of `before`, whether a line diff to `after` keeps it as
/// it is, by the diff read from the top and the one read from the bottom
/// (they break ties the other way); `None` when the texts differ too much
/// to tell.
fn unchanged_lines(before: &str, after: &str) -> Option<[Vec<bool>; 2]> {
    let old: Vec<&str> = before.split('\n').collect();
    let new: Vec<&str> = after.split('\n').collect();
    let from_top = kept_by(myers_edits(&old, &new)?);
    let (old_up, new_up): (Vec<&str>, Vec<&str>) = (
        old.iter().rev().copied().collect(),
        new.iter().rev().copied().collect(),
    );
    let mut from_bottom = kept_by(myers_edits(&old_up, &new_up)?);
    from_bottom.reverse();
    Some([from_top, from_bottom])
}

/// Whether each line of the old text is kept by `edits`.
fn kept_by(edits: Vec<DiffEdit>) -> Vec<bool> {
    edits
        .into_iter()
        .filter_map(|edit| match edit {
            DiffEdit::Equal => Some(true),
            DiffEdit::Delete => Some(false),
            DiffEdit::Insert => None,
        })
        .collect()
}

fn without_whitespace(units: &[u16]) -> Vec<u16> {
    units
        .iter()
        .copied()
        .filter(|unit| !is_blank(*unit))
        .collect()
}

/// The indent of each line holding code (not blank, not only a comment).
fn indents<'a>(behavior: &dyn ConfigLanguage, text: &'a str) -> Vec<&'a str> {
    text.lines()
        .filter_map(|line| {
            let indent = line.len() - line.trim_start().len();
            let code = !line.trim().is_empty() && behavior.comment_start(line) != Some(indent);
            code.then(|| &line[..indent])
        })
        .collect()
}
