//! The one rule, span by span: each span of the text before, where the
//! editor holds it after the edit, holds the text it held with nothing
//! glued onto its ends, and no locked field is lost.
//!
//! A locked field is held after the edit by the lock the locks of the text
//! after give it (placed by its path when the text parses, else followed),
//! and was moved there, when the edit kept its text, by `carry::Moves`
//! (the follower `locks::protect` uses). Its span is kept when either holds
//! its text: the lock placed by its path may stand on a copy, or take the
//! closer the edit added to a field a host text that never parsed left
//! open. Placed by its path and reading as pinned, a span whose solid text
//! is the same changed only whitespace inside it (an indent, a space after
//! `:` in a locked block): formatting, which the parse says leaves the
//! value as it was.

use std::cell::OnceCell;
use std::ops::Range;

use super::super::super::language::ConfigLanguage;
use super::super::super::paths::FieldPath;
use super::super::carry::Moves;
use super::super::locate::LineIndex;
use super::super::locks::{Lock, Protected};
use super::super::texts::Reason;
use super::closing::Closing;
use super::content::{content_areas, in_lock, in_locked_content};
use super::pins::all_pinned;
use super::spacing::between_tokens;
use super::stretch::{glued, is_blank};
use super::{Blocked, Carets, Why, refused};

/// Checks that every span of `before` (its locks `protected`, its host
/// ranges `ranges.0`) is kept in `after` (its locks `next`, the ranges
/// followed `ranges.1`); the locks were moved through the edit as
/// `carets` say.
pub(super) fn kept(
    behavior: &dyn ConfigLanguage,
    protected: &Protected,
    next: &Protected,
    (before, after): (&str, &str),
    (was, now): (&[Range<usize>], &[Range<usize>]),
    carets: Carets<'_>,
) -> Result<(), Blocked> {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    // Without a parse to compare, a character typed next to a YAML plain
    // value joins it, punctuation included (`abc<`, `2[x]`); so it does in
    // any language next to a lock with no pinned value (`2/ 3` in HCL).
    let strict = (behavior.plain_scalars() && next.document.is_none()) || !all_pinned(protected);
    let text = |units: &[u16], range: &Range<usize>| units.get(range.clone()).map(<[u16]>::to_vec);
    for (was, now) in was.iter().zip(now) {
        if text(&old, was) != text(&new, now) || glued(&old, &new, was, now, strict) {
            return Err(refused(&Reason::Range));
        }
    }
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old.len() - prefix)
        .min(new.len() - prefix);
    let spans = Spans {
        closing: Closing {
            behavior,
            before,
            old: &old,
            new: &new,
            region: prefix..old.len() - suffix,
            new_region: prefix..new.len() - suffix,
        },
        behavior,
        before,
        after,
        old: &old,
        new: &new,
        locks: &protected.locks,
        next,
        strict,
        lines: OnceCell::new(),
        content: OnceCell::new(),
    };
    let moves = Moves::at(before, after, carets.written, carets.change);
    for lock in protected.locks.iter().filter(|lock| lock.pointer.is_some()) {
        spans.lock_kept(protected, lock, &moves)?;
    }
    // A field held the text no longer has (its lock gone with its line)
    // stays held, locked, or kept unplaced (pinned, with no span to
    // lock): released, nothing would follow it.
    let locked = |pointer: &str| {
        protected
            .locks
            .iter()
            .any(|lock| lock.pointer.as_deref() == Some(pointer))
    };
    if let Some(lost) = protected.held.iter().find(|pointer| {
        !locked(pointer)
            && !next.held.contains(pointer)
            && !next.unplaced.iter().any(|(known, _)| known == *pointer)
            && next.locks.iter().all(|lock| {
                lock.pointer
                    .as_deref()
                    .is_none_or(|known| !named(behavior, (before, after), known, pointer))
            })
    }) {
        return Err(refused(&protected.held_reason(lost)));
    }
    Ok(())
}

/// The two texts and the locks after, as the checks of one lock read them.
struct Spans<'a> {
    closing: Closing<'a>,
    behavior: &'a dyn ConfigLanguage,
    before: &'a str,
    after: &'a str,
    old: &'a [u16],
    new: &'a [u16],
    /// The locks before.
    locks: &'a [Lock],
    next: &'a Protected,
    strict: bool,
    /// The lines of both texts, read when a span's spacing changed.
    lines: OnceCell<(LineIndex<'a>, LineIndex<'a>)>,
    /// Where the text before is content (see `content`), read when
    /// whitespace changed at or inside a span.
    content: OnceCell<Vec<Range<usize>>>,
}

impl Spans<'_> {
    /// Whether `lock` (of the text before) is kept by the lock the text
    /// after holds it by, or by its span moved through the edit.
    fn lock_kept(
        &self,
        protected: &Protected,
        lock: &Lock,
        moves: &Moves<'_>,
    ) -> Result<(), Blocked> {
        let pointer = lock.pointer.as_deref().unwrap_or_default();
        let placed = self.placed(pointer);
        let followed = moves.lock(lock);
        let held = self.next.held.iter().any(|known| known == pointer);
        let kept = |now: &Lock| self.same_text(lock, now) || self.respaced(lock, now);
        // A list item sits at another index when items before it went (a
        // field a host range covers is found again from the range there):
        // when the text after no longer holds its pointer, a lock holding
        // its text keeps it: the host's range itself, or, in a text that
        // parses, the lock of another pointer.
        let by_text = || {
            Some(self.next.locks.iter().find(|other| {
                (self.next.document.is_some() || other.pointer.is_none())
                    && self.same_value_text(lock, other)
            }))
        };
        // A text that does not parse holds its locks where the follower
        // moved them: a lock read again off it (the same text found on a
        // copy of its line) is not the span the edit kept.
        let parses = self.next.document.is_some();
        let chosen = placed
            .filter(|now| parses && kept(now))
            .or_else(|| followed.as_ref().filter(|now| kept(now)))
            .or_else(|| placed.is_none().then(by_text).flatten().flatten());
        let kept = chosen.filter(|now| !self.glued(lock, now));
        // A field of a host text that never parsed: its span, read off the
        // broken text, may run past it or stop short of its value. The
        // edit making the text parse is taken when its span is kept, or
        // when it only closes the field (`closing`).
        let first_parse = protected.document.is_none() && protected.parsed.is_none();
        let closed = || {
            let pin = protected
                .pins
                .iter()
                .find(|(known, _)| known == pointer)
                .map(|(_, value)| value);
            self.next
                .document
                .as_ref()
                .is_some_and(|edited| self.closing.only_closes(lock, pin, edited))
        };
        let Some(now) = kept.or_else(|| placed.filter(|_| first_parse && closed())) else {
            return Err(refused(&lock.reason));
        };
        if lock.reason != Reason::Range && !self.guards_kept(lock, now) {
            return Err(Blocked {
                reason: lock.reason.clone(),
                why: Why::Holds,
            });
        }
        // A field a host range covers is found again from the range in a
        // text that parses (the range keeps its own text): never lost there.
        let found_again = lock.reason == Reason::Range && self.next.document.is_some();
        let lost = placed.is_none()
            && !found_again
            && by_text().flatten().is_none()
            && !held
            && !self
                .next
                .unplaced
                .iter()
                .any(|(known, _)| self.named(known, pointer));
        if lost {
            return Err(refused(&lock.reason));
        }
        Ok(())
    }

    /// Whether `known` (a field of the text after) is the field at
    /// `pointer` of the text before (see [`named`]).
    fn named(&self, known: &str, pointer: &str) -> bool {
        named(self.behavior, (self.before, self.after), known, pointer)
    }

    /// The lock the text after holds the field at `pointer` by (under the
    /// other pointer of an XML element read as a list of one or alone).
    fn placed(&self, pointer: &str) -> Option<&Lock> {
        self.next.locks.iter().find(|lock| {
            lock.pointer
                .as_deref()
                .is_some_and(|known| self.named(known, pointer))
        })
    }

    /// The spans whose text `lock` keeps: its value's stretches (past its
    /// key for a field a host range covers, which keeps only the range's
    /// text) and its own key.
    fn own_spans(lock: &Lock) -> Vec<Range<usize>> {
        let range = lock.reason == Reason::Range;
        let value = |stretch: &Range<usize>| match &lock.key {
            Some(key)
                if range
                    && key.start <= stretch.start
                    && stretch.start < key.end
                    && key.end < stretch.end =>
            {
                key.end..stretch.end
            }
            _ => stretch.clone(),
        };
        let mut spans: Vec<Range<usize>> = lock.ranges.iter().map(value).collect();
        if !range {
            spans.extend(lock.key.clone());
        }
        spans
    }

    fn texts<'u>(units: &'u [u16], spans: &[Range<usize>]) -> Vec<&'u [u16]> {
        spans
            .iter()
            .map(|span| units.get(span.clone()).unwrap_or_default())
            .collect()
    }

    /// Whether `now` (of the text after) holds `was`'s text.
    fn same_text(&self, was: &Lock, now: &Lock) -> bool {
        Self::texts(self.old, &Self::own_spans(was)) == Self::texts(self.new, &Self::own_spans(now))
    }

    /// Whether `now`'s value stretches hold `was`'s value text (its key
    /// left out: a list item has none, a range none of its own).
    fn same_value_text(&self, was: &Lock, now: &Lock) -> bool {
        let value = |units: &[u16], lock: &Lock| {
            let mut spans = Self::own_spans(lock);
            if lock.reason != Reason::Range && lock.key.is_some() {
                spans.pop();
            }
            Self::texts(units, &spans).concat()
        };
        let text = value(self.old, was);
        !text.is_empty() && text == value(self.new, now)
    }

    /// Whether `now` holds `was`'s text but for whitespace between its
    /// tokens (see `spacing`): an indent where indentation carries no
    /// structure, a space after `:`, a line broken between two items;
    /// never in content (a string, a block scalar, an XML text).
    fn respaced(&self, was: &Lock, now: &Lock) -> bool {
        let solid = |units: &[u16], lock: &Lock| -> Vec<u16> {
            Self::texts(units, &Self::own_spans(lock))
                .concat()
                .into_iter()
                .filter(|unit| !is_blank(*unit))
                .collect()
        };
        let (Some(old), Some(new)) = (hull(&Self::own_spans(was)), hull(&Self::own_spans(now)))
        else {
            return false;
        };
        if solid(self.old, was) != solid(self.new, now) {
            return false;
        }
        let (old_lines, new_lines) = self
            .lines
            .get_or_init(|| (LineIndex::new(self.before), LineIndex::new(self.after)));
        let whole = |lines: &LineIndex<'_>, span: &Range<usize>| {
            let start = lines.line_bounds(lines.line_of(span.start)).start;
            let end = lines
                .line_bounds(lines.line_of(span.end.saturating_sub(1).max(span.start)))
                .end;
            (start, end)
        };
        let (start, end) = whole(old_lines, &old);
        let (new_start, new_end) = whole(new_lines, &new);
        let old_text = String::from_utf16_lossy(&self.old[start..end]);
        let new_text = String::from_utf16_lossy(&self.new[new_start..new_end]);
        // Each character's UTF-16 offset in the text before.
        let offsets: Vec<usize> = old_text
            .chars()
            .scan(start, |at, character| {
                let here = *at;
                *at += character.len_utf16();
                Some(here)
            })
            .chain([end])
            .collect();
        let content = |from: usize, to: usize| {
            in_locked_content(self.content_areas(), self.locks, offsets[from], offsets[to])
        };
        between_tokens(self.behavior, &old_text, &new_text, &content)
    }

    /// Where the text before is content, read once.
    fn content_areas(&self) -> &[Range<usize>] {
        self.content.get_or_init(|| {
            let (lines, _) = self
                .lines
                .get_or_init(|| (LineIndex::new(self.before), LineIndex::new(self.after)));
            content_areas(self.behavior, self.before, lines)
        })
    }

    /// Whether whitespace written at `at` (an end of a span of `lock`,
    /// UTF-16 in the text before) is the span's content: `at` lies in
    /// content that starts in the lock (the last line of a block scalar).
    fn in_content(&self, lock: &Lock, at: usize) -> bool {
        self.content_areas()
            .iter()
            .any(|area| area.start <= at && at <= area.end && in_lock(area, lock))
    }

    /// Whether text was glued onto an end of `was`'s spans, where `now`
    /// holds them: stretch by stretch when they are as many, else at the
    /// ends of all of them.
    fn glued(&self, was: &Lock, now: &Lock) -> bool {
        let (old, new) = (Self::own_spans(was), Self::own_spans(now));
        let pairs: Vec<(Range<usize>, Range<usize>)> = if old.len() == new.len() {
            old.into_iter().zip(new).collect()
        } else {
            hull(&old).zip(hull(&new)).into_iter().collect()
        };
        pairs.iter().any(|(span, moved)| {
            glued(self.old, self.new, span, moved, self.strict)
                || self.spaced_in_content(was, span, moved)
        })
    }

    /// Whether whitespace was written at an end of `span` (of `lock`, now
    /// at `moved`) where it is content (see [`Self::in_content`]).
    fn spaced_in_content(&self, lock: &Lock, span: &Range<usize>, moved: &Range<usize>) -> bool {
        let newly_blank = |old: Option<&u16>, new: Option<&u16>| {
            old != new && new.is_some_and(|unit| is_blank(*unit))
        };
        let before = newly_blank(
            span.start.checked_sub(1).and_then(|at| self.old.get(at)),
            moved.start.checked_sub(1).and_then(|at| self.new.get(at)),
        );
        let after = newly_blank(self.old.get(span.end), self.new.get(moved.end));
        (before && self.in_content(lock, span.start)) || (after && self.in_content(lock, span.end))
    }

    /// Whether the keys holding `was` hold it still where `now` is: the
    /// same keys, none glued onto.
    fn guards_kept(&self, was: &Lock, now: &Lock) -> bool {
        Self::texts(self.old, &was.guards) == Self::texts(self.new, &now.guards)
            && was
                .guards
                .iter()
                .zip(&now.guards)
                .all(|(was, now)| !glued(self.old, self.new, was, now, self.strict))
    }
}

/// Whether `known` (a field of `after`) names the field at `pointer` (of
/// `before`, the text an edit made `after` of): the same pointer, or the
/// same item of a list written once in one text (a lone XML element or HCL
/// block, `/l`) and repeated in the other (`/l/0`), as the language reads
/// a value written once (see `data_reading::lone_spelling`).
pub(in crate::editor::protect) fn named(
    behavior: &dyn ConfigLanguage,
    (before, after): (&str, &str),
    known: &str,
    pointer: &str,
) -> bool {
    let lone = |text: &str, written: &str| behavior.lone_item(text, &FieldPath::from(written));
    known == pointer
        || (known.strip_suffix("/0") == Some(pointer) && lone(before, pointer))
        || (pointer.strip_suffix("/0") == Some(known) && lone(after, known))
}

/// From the first span's start to the last one's end.
fn hull(spans: &[Range<usize>]) -> Option<Range<usize>> {
    let start = spans.iter().map(|span| span.start).min()?;
    let end = spans.iter().map(|span| span.end).max()?;
    Some(start..end)
}
